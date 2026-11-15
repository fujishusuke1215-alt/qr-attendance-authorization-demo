use std::{env, error::Error, fmt, sync::Arc};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

const MIN_KEY_BYTES: usize = 32;
const TOKEN_VERSION: &str = "qra1";
type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QrKind {
    CheckIn,
    CheckOut,
}

impl QrKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CheckIn => "check_in",
            Self::CheckOut => "check_out",
        }
    }
}

/// HMAC key wrapper deliberately has no Debug or Display implementation.
#[derive(Clone, PartialEq, Eq)]
pub struct QrSigner {
    key: Arc<[u8]>,
    site_id: Arc<str>,
}

impl QrSigner {
    pub fn from_env(site_id: impl Into<String>) -> Result<Self, QrTokenError> {
        let encoded = env::var("QR_HMAC_KEY_BASE64").map_err(|_| QrTokenError::MissingKey)?;
        Self::from_base64(&encoded, site_id)
    }

    pub fn from_base64(encoded: &str, site_id: impl Into<String>) -> Result<Self, QrTokenError> {
        let key = base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .map_err(|_| QrTokenError::InvalidKey)?;
        if key.len() < MIN_KEY_BYTES {
            return Err(QrTokenError::InvalidKey);
        }
        let site_id = site_id.into();
        if site_id.trim().is_empty() || site_id.contains('|') {
            return Err(QrTokenError::InvalidContext);
        }
        Ok(Self {
            key: Arc::from(key),
            site_id: Arc::from(site_id),
        })
    }

    /// A printed code remains stable until its admin-managed revision changes.
    pub fn derive(&self, kind: QrKind, revision: i64) -> String {
        let message = format!(
            "{TOKEN_VERSION}|{}|{}|{revision}",
            self.site_id,
            kind.as_str()
        );
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&self.key)
            .expect("HMAC-SHA256 accepts keys of any length");
        mac.update(message.as_bytes());
        let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        format!(
            "{TOKEN_VERSION}.{}.{}.{}",
            kind.as_str(),
            revision,
            signature
        )
    }

    /// Verifies the token and requires its revision to still be the active one.
    pub fn verify(&self, token: &str, expected_kind: QrKind, current_revision: i64) -> bool {
        let mut parts = token.split('.');
        let (Some(version), Some(kind), Some(revision), Some(encoded), None) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            return false;
        };
        if version != TOKEN_VERSION || kind != expected_kind.as_str() {
            return false;
        }
        let Ok(revision) = revision.parse::<i64>() else {
            return false;
        };
        if revision != current_revision || revision <= 0 {
            return false;
        }
        let Ok(signature) = URL_SAFE_NO_PAD.decode(encoded) else {
            return false;
        };
        let message = format!("{TOKEN_VERSION}|{}|{}|{revision}", self.site_id, kind);
        let Ok(mut mac) = <HmacSha256 as Mac>::new_from_slice(&self.key) else {
            return false;
        };
        mac.update(message.as_bytes());
        mac.verify_slice(&signature).is_ok()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QrTokenError {
    MissingKey,
    InvalidKey,
    InvalidContext,
}

impl fmt::Display for QrTokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::MissingKey => "QR_HMAC_KEY_BASE64 is required",
            Self::InvalidKey => {
                "QR_HMAC_KEY_BASE64 must be valid base64 containing at least 32 bytes"
            }
            Self::InvalidContext => "QR site identifier is invalid",
        })
    }
}
impl Error for QrTokenError {}

#[cfg(test)]
mod tests {
    use super::{QrKind, QrSigner};

    const KEY_A: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    const KEY_B: &str = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=";

    #[test]
    fn token_is_deterministic_for_same_inputs() {
        let signer = QrSigner::from_base64(KEY_A, "tokyo-office").unwrap();
        assert_eq!(
            signer.derive(QrKind::CheckIn, 7),
            signer.derive(QrKind::CheckIn, 7)
        );
        assert!(signer.verify(&signer.derive(QrKind::CheckIn, 7), QrKind::CheckIn, 7));
    }

    #[test]
    fn token_changes_with_key_kind_site_or_revision() {
        let signer = QrSigner::from_base64(KEY_A, "tokyo-office").unwrap();
        let base = signer.derive(QrKind::CheckIn, 7);
        assert_ne!(
            base,
            QrSigner::from_base64(KEY_B, "tokyo-office")
                .unwrap()
                .derive(QrKind::CheckIn, 7)
        );
        assert_ne!(base, signer.derive(QrKind::CheckOut, 7));
        assert_ne!(
            base,
            QrSigner::from_base64(KEY_A, "osaka-office")
                .unwrap()
                .derive(QrKind::CheckIn, 7)
        );
        assert_ne!(base, signer.derive(QrKind::CheckIn, 8));
    }

    #[test]
    fn invalid_key_and_token_are_rejected_without_secret_output() {
        let secret = "do-not-print-this-key";
        let error = QrSigner::from_base64(secret, "tokyo-office").err().unwrap();
        assert_eq!(
            error.to_string(),
            "QR_HMAC_KEY_BASE64 must be valid base64 containing at least 32 bytes"
        );
        assert!(!format!("{error:?}").contains(secret));
        let signer = QrSigner::from_base64(KEY_A, "tokyo-office").unwrap();
        assert!(!signer.verify("qra1.check_in.7.bad-signature", QrKind::CheckIn, 7));
        assert!(!signer.verify(&signer.derive(QrKind::CheckIn, 7), QrKind::CheckIn, 8));
        assert!(!signer.verify(&signer.derive(QrKind::CheckIn, 7), QrKind::CheckOut, 7));
    }
}
