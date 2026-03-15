//! テナントシークレットの暗号化・復号
//!
//! AES-256-GCM で暗号化し、base64 エンコードして DB に保存する。
//! マスターキーは環境変数 `FLEETFLOW_MASTER_KEY`（64文字 hex = 32バイト）から取得。
//!
//! フォーマット: base64(nonce[12] || ciphertext || tag[16])

use aes_gcm::{
    AeadCore, Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, OsRng},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};

/// マスターキーの環境変数名
pub const MASTER_KEY_ENV: &str = "FLEETFLOW_MASTER_KEY";

/// 暗号化エラー
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("マスターキーが未設定（{MASTER_KEY_ENV}）")]
    MasterKeyNotSet,

    #[error("マスターキーが不正（64文字の hex 文字列が必要）")]
    InvalidMasterKey,

    #[error("暗号化失敗: {0}")]
    EncryptFailed(String),

    #[error("復号失敗（キーが異なるかデータが破損）")]
    DecryptFailed,

    #[error("base64 デコード失敗: {0}")]
    Base64Error(#[from] base64::DecodeError),

    #[error("データが短すぎます（nonce + tag に満たない）")]
    DataTooShort,
}

type Result<T> = std::result::Result<T, CryptoError>;

/// 環境変数からマスターキーを取得
pub fn load_master_key() -> Result<[u8; 32]> {
    let hex = std::env::var(MASTER_KEY_ENV).map_err(|_| CryptoError::MasterKeyNotSet)?;
    parse_hex_key(&hex)
}

/// hex 文字列を 32 バイトキーにパース
fn parse_hex_key(hex: &str) -> Result<[u8; 32]> {
    if hex.len() != 64 {
        return Err(CryptoError::InvalidMasterKey);
    }
    let mut key = [0u8; 32];
    for i in 0..32 {
        key[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
            .map_err(|_| CryptoError::InvalidMasterKey)?;
    }
    Ok(key)
}

/// 平文を暗号化して base64 文字列を返す
pub fn encrypt(plaintext: &str, master_key: &[u8; 32]) -> Result<String> {
    let cipher = Aes256Gcm::new(master_key.into());
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| CryptoError::EncryptFailed(e.to_string()))?;

    // nonce(12) + ciphertext + tag(16, appended by aes-gcm)
    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce);
    combined.extend_from_slice(&ciphertext);

    Ok(BASE64.encode(&combined))
}

/// base64 文字列を復号して平文を返す
pub fn decrypt(encrypted: &str, master_key: &[u8; 32]) -> Result<String> {
    let combined = BASE64.decode(encrypted)?;

    // nonce(12) + ciphertext(n) + tag(16, included in ciphertext by aes-gcm)
    if combined.len() < 12 + 16 {
        return Err(CryptoError::DataTooShort);
    }

    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = Aes256Gcm::new(master_key.into());
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::DecryptFailed)?;

    String::from_utf8(plaintext).map_err(|_| CryptoError::DecryptFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        // deterministic test key
        let hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        parse_hex_key(hex).unwrap()
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = test_key();
        let plaintext = "my-secret-api-token-12345";

        let encrypted = encrypt(plaintext, &key).unwrap();
        assert_ne!(encrypted, plaintext);

        let decrypted = decrypt(&encrypted, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_produces_different_output() {
        let key = test_key();
        let plaintext = "same-input";

        let enc1 = encrypt(plaintext, &key).unwrap();
        let enc2 = encrypt(plaintext, &key).unwrap();

        // ランダム nonce により毎回異なる暗号文
        assert_ne!(enc1, enc2);

        // どちらも正しく復号できる
        assert_eq!(decrypt(&enc1, &key).unwrap(), plaintext);
        assert_eq!(decrypt(&enc2, &key).unwrap(), plaintext);
    }

    #[test]
    fn test_wrong_key_fails() {
        let key1 = test_key();
        let mut key2 = test_key();
        key2[0] ^= 0xff; // 1 バイト変える

        let encrypted = encrypt("secret", &key1).unwrap();
        let result = decrypt(&encrypted, &key2);

        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_data_fails() {
        let key = test_key();
        let encrypted = encrypt("secret", &key).unwrap();

        let mut bytes = BASE64.decode(&encrypted).unwrap();
        bytes[15] ^= 0xff; // ciphertext の一部を改ざん
        let tampered = BASE64.encode(&bytes);

        assert!(decrypt(&tampered, &key).is_err());
    }

    #[test]
    fn test_empty_string() {
        let key = test_key();
        let encrypted = encrypt("", &key).unwrap();
        let decrypted = decrypt(&encrypted, &key).unwrap();
        assert_eq!(decrypted, "");
    }

    #[test]
    fn test_parse_hex_key_valid() {
        let hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let key = parse_hex_key(hex).unwrap();
        assert_eq!(key[0], 0x01);
        assert_eq!(key[1], 0x23);
        assert_eq!(key[31], 0xef);
    }

    #[test]
    fn test_parse_hex_key_wrong_length() {
        assert!(parse_hex_key("0123").is_err());
        assert!(parse_hex_key("").is_err());
    }

    #[test]
    fn test_parse_hex_key_invalid_chars() {
        let hex = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
        assert!(parse_hex_key(hex).is_err());
    }

    #[test]
    fn test_data_too_short() {
        let key = test_key();
        let short = BASE64.encode(&[0u8; 10]);
        assert!(matches!(
            decrypt(&short, &key),
            Err(CryptoError::DataTooShort)
        ));
    }
}
