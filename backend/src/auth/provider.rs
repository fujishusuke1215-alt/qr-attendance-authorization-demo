use async_trait::async_trait;
use axum::http::HeaderMap;
use thiserror::Error;

use super::EntraAuthenticator;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthMode {
    Development,
    Entra,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Principal {
    /// Stable upstream subject: a demo UUID or Entra object ID.
    pub subject: String,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AuthenticationFailure {
    #[error("credentials are missing or invalid")]
    InvalidCredentials,
    #[error("identity provider could not be reached")]
    ProviderUnavailable,
}

#[async_trait]
pub trait Authenticator: Send + Sync {
    async fn authenticate(&self, headers: &HeaderMap) -> Result<Principal, AuthenticationFailure>;
}

pub struct DevelopmentAuthenticator;

#[async_trait]
impl Authenticator for DevelopmentAuthenticator {
    async fn authenticate(&self, headers: &HeaderMap) -> Result<Principal, AuthenticationFailure> {
        let value = headers
            .get("x-dev-user")
            .and_then(|value| value.to_str().ok())
            .ok_or(AuthenticationFailure::InvalidCredentials)?;
        uuid::Uuid::parse_str(value).map_err(|_| AuthenticationFailure::InvalidCredentials)?;
        Ok(Principal {
            subject: value.to_owned(),
        })
    }
}

pub fn build_authenticator(
    mode: AuthMode,
    tenant_id: Option<String>,
    audience: Option<String>,
) -> Result<Box<dyn Authenticator>, String> {
    match mode {
        AuthMode::Development => Ok(Box::new(DevelopmentAuthenticator)),
        AuthMode::Entra => {
            let tenant = tenant_id
                .ok_or_else(|| "ENTRA_TENANT_ID is required when AUTH_MODE=entra".to_string())?;
            let audience = audience
                .ok_or_else(|| "ENTRA_API_AUDIENCE is required when AUTH_MODE=entra".to_string())?;
            uuid::Uuid::parse_str(&tenant)
                .map_err(|_| "ENTRA_TENANT_ID must be a tenant GUID".to_string())?;
            if audience.trim().is_empty() {
                return Err("ENTRA_API_AUDIENCE must not be empty".into());
            }
            Ok(Box::new(EntraAuthenticator::new(tenant, audience)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AuthMode, Authenticator, DevelopmentAuthenticator, build_authenticator};
    use axum::http::{HeaderMap, HeaderValue};

    #[tokio::test]
    async fn development_adapter_requires_a_valid_demo_identity() {
        let authenticator = DevelopmentAuthenticator;
        assert!(authenticator.authenticate(&HeaderMap::new()).await.is_err());
        let mut headers = HeaderMap::new();
        headers.insert("x-dev-user", HeaderValue::from_static("not-a-uuid"));
        assert!(authenticator.authenticate(&headers).await.is_err());
        headers.insert(
            "x-dev-user",
            HeaderValue::from_static("10000000-0000-4000-8000-000000000001"),
        );
        assert!(authenticator.authenticate(&headers).await.is_ok());
    }

    #[test]
    fn dev_auth_is_disabled_outside_development_mode() {
        assert!(build_authenticator(AuthMode::Development, None, None).is_ok());
        assert!(build_authenticator(AuthMode::Entra, None, None).is_err());
    }
}
