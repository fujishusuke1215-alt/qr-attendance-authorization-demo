use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    auth::{AuthMode, Authenticator},
    error::AppError,
    qr::QrSigner,
    sync::{HrSource, SyncService},
};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub auth_mode: AuthMode,
    pub authenticator: Arc<dyn Authenticator>,
    pub signer: QrSigner,
    pub public_app_url: Arc<str>,
    pub sync: SyncService,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        auth_mode: AuthMode,
        authenticator: Box<dyn Authenticator>,
        signer: QrSigner,
        public_app_url: String,
        source: Arc<dyn HrSource>,
    ) -> Self {
        let sync = SyncService::new(pool.clone(), source.clone());
        Self {
            pool,
            auth_mode,
            authenticator: Arc::from(authenticator),
            signer,
            public_app_url: Arc::from(public_app_url),
            sync,
        }
    }

    pub fn allowed_origin(&self) -> axum::http::HeaderValue {
        axum::http::HeaderValue::from_str(self.public_app_url.trim_end_matches('/'))
            .unwrap_or_else(|_| axum::http::HeaderValue::from_static("http://localhost:5173"))
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CurrentUser {
    pub id: Uuid,
    pub display_name: String,
    pub email: Option<String>,
    pub employee_code: Option<String>,
    pub job_code: Option<String>,
}

impl CurrentUser {
    pub fn has_complete_attendance_profile(&self) -> bool {
        !self.display_name.trim().is_empty()
            && self
                .email
                .as_deref()
                .is_some_and(|email| email.contains('@'))
            && self
                .employee_code
                .as_deref()
                .is_some_and(|code| !code.trim().is_empty())
    }
}

#[async_trait]
pub trait UserDirectory: Send + Sync {
    async fn find_dev(&self, id: Uuid) -> Result<Option<CurrentUser>, sqlx::Error>;
    async fn find_entra(&self, object_id: &str) -> Result<Option<CurrentUser>, sqlx::Error>;
}

#[async_trait]
impl UserDirectory for PgPool {
    async fn find_dev(&self, id: Uuid) -> Result<Option<CurrentUser>, sqlx::Error> {
        user_row(sqlx::query("SELECT id, display_name, email, employee_code, job_code FROM users WHERE id = $1 AND active = TRUE").bind(id).fetch_optional(self).await?)
    }

    async fn find_entra(&self, object_id: &str) -> Result<Option<CurrentUser>, sqlx::Error> {
        user_row(sqlx::query("SELECT id, display_name, email, employee_code, job_code FROM users WHERE entra_oid = $1 AND active = TRUE").bind(object_id).fetch_optional(self).await?)
    }
}

fn user_row(row: Option<sqlx::postgres::PgRow>) -> Result<Option<CurrentUser>, sqlx::Error> {
    row.map(|row| {
        Ok(CurrentUser {
            id: row.try_get("id")?,
            display_name: row.try_get("display_name")?,
            email: row.try_get("email")?,
            employee_code: row.try_get("employee_code")?,
            job_code: row.try_get("job_code")?,
        })
    })
    .transpose()
}

pub async fn authenticate(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let principal = match state.authenticator.authenticate(request.headers()).await {
        Ok(principal) => principal,
        Err(crate::auth::AuthenticationFailure::InvalidCredentials) => {
            return AppError::Unauthenticated.into_response();
        }
        Err(crate::auth::AuthenticationFailure::ProviderUnavailable) => {
            return AppError::AuthUnavailable.into_response();
        }
    };
    let user = match state.auth_mode {
        AuthMode::Development => match Uuid::parse_str(&principal.subject) {
            Ok(id) => state.pool.find_dev(id).await,
            Err(_) => return AppError::Unauthenticated.into_response(),
        },
        AuthMode::Entra => state.pool.find_entra(&principal.subject).await,
    };
    match user {
        Ok(Some(user)) => {
            request.extensions_mut().insert(user);
            next.run(request).await
        }
        Ok(None) => AppError::Unauthenticated.into_response(),
        Err(_) => AppError::Internal.into_response(),
    }
}

pub async fn require_admin(state: &AppState, user: &CurrentUser) -> Result<(), AppError> {
    let is_admin = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM user_roles WHERE user_id = $1 AND role = 'admin')",
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::Internal)?;
    if is_admin {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

pub async fn is_admin(state: &AppState, user: &CurrentUser) -> Result<bool, AppError> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM user_roles WHERE user_id = $1 AND role = 'admin')",
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::Internal)
}
