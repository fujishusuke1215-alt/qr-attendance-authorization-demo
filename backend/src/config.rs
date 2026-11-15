use std::{env, net::SocketAddr};

use crate::{
    auth::AuthMode,
    qr::{QrSigner, QrTokenError},
};

pub struct Config {
    pub app_env: String,
    pub auth_mode: AuthMode,
    pub database_url: String,
    pub listen: SocketAddr,
    pub public_app_url: String,
    pub entra_tenant_id: Option<String>,
    pub entra_api_audience: Option<String>,
    pub qr_signer: QrSigner,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let app_env = env::var("APP_ENV").unwrap_or_else(|_| "development".into());
        let auth_mode = match env::var("AUTH_MODE")
            .unwrap_or_else(|_| "dev".into())
            .as_str()
        {
            "dev" => AuthMode::Development,
            "entra" => AuthMode::Entra,
            other => return Err(format!("unsupported AUTH_MODE: {other}")),
        };
        if auth_mode == AuthMode::Development && app_env == "production" {
            return Err("development authentication cannot be enabled in production".into());
        }
        let host = env::var("API_HOST").unwrap_or_else(|_| "0.0.0.0".into());
        let port = env::var("API_PORT")
            .unwrap_or_else(|_| "8080".into())
            .parse::<u16>()
            .map_err(|_| "API_PORT must be a valid TCP port".to_string())?;
        let signer =
            QrSigner::from_env(env::var("QR_SITE_ID").unwrap_or_else(|_| "head-office".into()))
                .map_err(|error: QrTokenError| error.to_string())?;
        Ok(Self {
            app_env,
            auth_mode,
            database_url: env::var("DATABASE_URL")
                .map_err(|_| "DATABASE_URL is required".to_string())?,
            listen: format!("{host}:{port}")
                .parse()
                .map_err(|_| "API_HOST/API_PORT is invalid".to_string())?,
            public_app_url: env::var("PUBLIC_APP_URL")
                .unwrap_or_else(|_| "http://localhost:5173".into()),
            entra_tenant_id: env::var("ENTRA_TENANT_ID").ok(),
            entra_api_audience: env::var("ENTRA_API_AUDIENCE").ok(),
            qr_signer: signer,
        })
    }
}
