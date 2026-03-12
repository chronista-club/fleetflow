use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, Semaphore};
use tracing::{debug, info, warn};

/// JWKS キャッシュの TTL（1 時間）
const JWKS_CACHE_TTL: Duration = Duration::from_secs(3600);

/// kid 不一致時の再フェッチ抑制間隔（30 秒）
const JWKS_REFETCH_COOLDOWN: Duration = Duration::from_secs(30);

/// Auth0 JWT claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (Auth0 user ID)
    pub sub: String,
    /// Audience
    pub aud: ClaimAudience,
    /// Issuer
    pub iss: String,
    /// Expiration time (Unix timestamp)
    pub exp: u64,
    /// Issued at (Unix timestamp)
    pub iat: u64,
    /// Email (custom claim)
    pub email: Option<String>,
    /// Permissions (Auth0 RBAC)
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// Auth0 audience can be string or array.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ClaimAudience {
    Single(String),
    Multiple(Vec<String>),
}

/// JWKS key set from Auth0.
#[derive(Debug, Clone, Deserialize)]
pub struct JwkSet {
    pub keys: Vec<Jwk>,
}

/// Single JWK key.
#[derive(Debug, Clone, Deserialize)]
pub struct Jwk {
    pub kty: String,
    pub kid: Option<String>,
    pub n: Option<String>,
    pub e: Option<String>,
    pub alg: Option<String>,
    #[serde(rename = "use")]
    pub use_: Option<String>,
}

/// JWKS cache state (cache + cooldown tracking).
struct JwksCacheState {
    jwks: Option<JwkSet>,
    fetched_at: Option<Instant>,
    last_invalidation: Option<Instant>,
}

/// Auth0 JWT verifier with JWKS caching.
pub struct Auth0Verifier {
    jwks_uri: String,
    audience: String,
    issuer: String,
    cache: Arc<RwLock<JwksCacheState>>,
    fetch_semaphore: Semaphore,
    http_client: reqwest::Client,
}

/// Auth0 configuration.
#[derive(Debug)]
pub struct Auth0Config {
    pub domain: String,
    pub audience: String,
}

impl Auth0Verifier {
    pub fn new(config: &Auth0Config) -> Self {
        let issuer = format!("https://{}/", config.domain);
        let jwks_uri = format!("https://{}/.well-known/jwks.json", config.domain);

        Self {
            jwks_uri,
            audience: config.audience.clone(),
            issuer,
            cache: Arc::new(RwLock::new(JwksCacheState {
                jwks: None,
                fetched_at: None,
                last_invalidation: None,
            })),
            fetch_semaphore: Semaphore::new(1),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .connect_timeout(Duration::from_secs(5))
                .build()
                .expect("reqwest Client 作成失敗"),
        }
    }

    /// Auth0 ドメイン（issuer から復元）
    pub fn domain(&self) -> &str {
        self.issuer
            .strip_prefix("https://")
            .and_then(|s| s.strip_suffix('/'))
            .unwrap_or(&self.issuer)
    }

    /// Audience
    pub fn audience(&self) -> &str {
        &self.audience
    }

    /// Verify an access token and return claims.
    pub async fn verify(&self, token: &str) -> Result<Claims> {
        let header = jsonwebtoken::decode_header(token).context("JWT ヘッダーデコード失敗")?;

        let kid = header.kid.context("JWT に kid がありません")?;

        // Try cached JWKS first, retry with fresh JWKS on kid-not-found (key rotation)
        // Cooldown で thundering herd を防止
        let jwk = match self.find_jwk(&kid).await? {
            Some(jwk) => jwk,
            None => {
                warn!(kid = %kid, "JWK kid 不一致、キャッシュ更新して再試行");
                self.invalidate_cache_throttled().await;
                self.find_jwk(&kid)
                    .await?
                    .context("一致する JWK が見つかりません（キャッシュ更新後も不一致）")?
            }
        };

        let n = jwk.n.as_ref().context("JWK に n がありません")?;
        let e = jwk.e.as_ref().context("JWK に e がありません")?;

        let decoding_key =
            DecodingKey::from_rsa_components(n, e).context("RSA デコーディングキー作成失敗")?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[&self.audience]);
        validation.set_issuer(&[&self.issuer]);

        let token_data =
            decode::<Claims>(token, &decoding_key, &validation).context("JWT 検証失敗")?;

        debug!(sub = %token_data.claims.sub, "JWT 検証成功");
        Ok(token_data.claims)
    }

    /// Find a JWK by kid, filtering for signing keys (use=sig, alg=RS256).
    async fn find_jwk(&self, kid: &str) -> Result<Option<Jwk>> {
        let jwks = self.get_jwks().await?;
        Ok(jwks.keys.into_iter().find(|k| {
            k.kid.as_deref() == Some(kid)
                && k.use_.as_deref() == Some("sig")
                && k.alg.as_deref().is_none_or(|a| a == "RS256")
        }))
    }

    /// Fetch JWKS from Auth0 (with TTL-based caching).
    async fn get_jwks(&self) -> Result<JwkSet> {
        // Fast path: read lock でキャッシュ確認
        {
            let state = self.cache.read().await;
            if let Some(ref jwks) = state.jwks
                && let Some(fetched_at) = state.fetched_at
                && fetched_at.elapsed() < JWKS_CACHE_TTL
            {
                return Ok(jwks.clone());
            }
        }

        // Slow path: semaphore で HTTP fetch を1タスクに直列化
        let _permit = self
            .fetch_semaphore
            .acquire()
            .await
            .context("JWKS fetch semaphore 取得失敗")?;

        // Semaphore 取得後にキャッシュ再確認（先行タスクが更新済みの可能性）
        {
            let state = self.cache.read().await;
            if let Some(ref jwks) = state.jwks
                && let Some(fetched_at) = state.fetched_at
                && fetched_at.elapsed() < JWKS_CACHE_TTL
            {
                return Ok(jwks.clone());
            }
        }

        // Fetch from Auth0
        info!(uri = %self.jwks_uri, "JWKS 取得中");
        let jwks: JwkSet = self
            .http_client
            .get(&self.jwks_uri)
            .send()
            .await
            .context("JWKS リクエスト失敗")?
            .error_for_status()
            .context("JWKS HTTP エラー")?
            .json()
            .await
            .context("JWKS パース失敗")?;

        // Update cache with timestamp
        {
            let mut state = self.cache.write().await;
            state.jwks = Some(jwks.clone());
            state.fetched_at = Some(Instant::now());
        }

        info!(keys = jwks.keys.len(), "JWKS キャッシュ更新完了");
        Ok(jwks)
    }

    /// Invalidate JWKS cache with cooldown to prevent thundering herd.
    async fn invalidate_cache_throttled(&self) {
        let mut state = self.cache.write().await;
        if let Some(ref t) = state.last_invalidation
            && t.elapsed() < JWKS_REFETCH_COOLDOWN
        {
            debug!("JWKS 再フェッチ cooldown 中、スキップ");
            return;
        }
        state.jwks = None;
        state.fetched_at = None;
        state.last_invalidation = Some(Instant::now());
    }

    /// Invalidate JWKS cache (for key rotation).
    pub async fn invalidate_cache(&self) {
        let mut state = self.cache.write().await;
        state.jwks = None;
        state.fetched_at = None;
    }
}
