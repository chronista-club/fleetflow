use anyhow::{Context, Result};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

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

/// Auth0 JWT verifier with JWKS caching.
pub struct Auth0Verifier {
    jwks_uri: String,
    audience: String,
    issuer: String,
    jwks_cache: Arc<RwLock<Option<JwkSet>>>,
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
            jwks_cache: Arc::new(RwLock::new(None)),
            http_client: reqwest::Client::new(),
        }
    }

    /// Verify an access token and return claims.
    pub async fn verify(&self, token: &str) -> Result<Claims> {
        let header = jsonwebtoken::decode_header(token).context("JWT ヘッダーデコード失敗")?;

        let kid = header.kid.context("JWT に kid がありません")?;

        let jwks = self.get_jwks().await?;
        let jwk = jwks
            .keys
            .iter()
            .find(|k| k.kid.as_deref() == Some(kid.as_str()))
            .context("一致する JWK が見つかりません")?;

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

    /// Fetch JWKS from Auth0 (with caching).
    async fn get_jwks(&self) -> Result<JwkSet> {
        // Check cache first
        {
            let cache = self.jwks_cache.read().await;
            if let Some(ref jwks) = *cache {
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
            .json()
            .await
            .context("JWKS パース失敗")?;

        // Update cache
        {
            let mut cache = self.jwks_cache.write().await;
            *cache = Some(jwks.clone());
        }

        info!(keys = jwks.keys.len(), "JWKS キャッシュ更新完了");
        Ok(jwks)
    }

    /// Invalidate JWKS cache (for key rotation).
    pub async fn invalidate_cache(&self) {
        let mut cache = self.jwks_cache.write().await;
        *cache = None;
    }
}
