use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use axum::http::HeaderMap;
use jsonwebtoken::{
    Algorithm, DecodingKey, Header, Validation, decode, decode_header, jwk::JwkSet,
};
use serde::Deserialize;
use tokio::sync::RwLock;
use tokio::time::Instant;

use super::provider::{AuthenticationFailure, Authenticator, Principal};

pub struct EntraAuthenticator {
    tenant_id: Arc<str>,
    audience: Arc<str>,
    client: reqwest::Client,
    cached_keys: RwLock<Option<(Instant, JwkSet)>>,
}

impl EntraAuthenticator {
    pub fn new(tenant_id: String, audience: String) -> Self {
        Self {
            tenant_id: Arc::from(tenant_id),
            audience: Arc::from(audience),
            client: reqwest::Client::new(),
            cached_keys: RwLock::new(None),
        }
    }

    fn issuer(&self) -> String {
        format!("https://login.microsoftonline.com/{}/v2.0", self.tenant_id)
    }

    async fn signing_keys(&self) -> Result<JwkSet, AuthenticationFailure> {
        if let Some((expires, keys)) = self.cached_keys.read().await.as_ref()
            && *expires > Instant::now()
        {
            return Ok(keys.clone());
        }
        let url = format!(
            "https://login.microsoftonline.com/{}/discovery/v2.0/keys",
            self.tenant_id
        );
        let keys = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| AuthenticationFailure::ProviderUnavailable)?
            .error_for_status()
            .map_err(|_| AuthenticationFailure::ProviderUnavailable)?
            .json::<JwkSet>()
            .await
            .map_err(|_| AuthenticationFailure::ProviderUnavailable)?;
        *self.cached_keys.write().await =
            Some((Instant::now() + Duration::from_secs(3600), keys.clone()));
        Ok(keys)
    }

    async fn verify_token(&self, token: &str) -> Result<EntraClaims, AuthenticationFailure> {
        let header: Header =
            decode_header(token).map_err(|_| AuthenticationFailure::InvalidCredentials)?;
        if header.alg != Algorithm::RS256 {
            return Err(AuthenticationFailure::InvalidCredentials);
        }
        let kid = header
            .kid
            .as_deref()
            .ok_or(AuthenticationFailure::InvalidCredentials)?;
        let keys = self.signing_keys().await?;
        let jwk = keys
            .keys
            .iter()
            .find(|key| key.common.key_id.as_deref() == Some(kid))
            .ok_or(AuthenticationFailure::InvalidCredentials)?;
        let decoding_key =
            DecodingKey::from_jwk(jwk).map_err(|_| AuthenticationFailure::InvalidCredentials)?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[self.issuer()]);
        validation.set_audience(&[self.audience.as_ref()]);
        validation.validate_nbf = true;
        let claims = decode::<EntraClaims>(token, &decoding_key, &validation)
            .map_err(|_| AuthenticationFailure::InvalidCredentials)?
            .claims;
        if claims.oid.trim().is_empty() {
            return Err(AuthenticationFailure::InvalidCredentials);
        }
        Ok(claims)
    }
}

#[derive(Debug, Deserialize)]
struct EntraClaims {
    oid: String,
    exp: usize,
    iss: String,
    aud: String,
    #[serde(default)]
    nbf: usize,
}

#[async_trait]
impl Authenticator for EntraAuthenticator {
    async fn authenticate(&self, headers: &HeaderMap) -> Result<Principal, AuthenticationFailure> {
        let authorization = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .ok_or(AuthenticationFailure::InvalidCredentials)?;
        let claims = self.verify_token(authorization).await?;
        let _validated_standard_claims = (claims.exp, claims.nbf, claims.iss, claims.aud);
        Ok(Principal {
            subject: claims.oid,
        })
    }
}
