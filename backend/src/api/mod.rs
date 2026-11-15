mod admin;
mod attendance;
mod auth;
mod qr;
mod state;
mod sync;

use axum::{
    Json, Router,
    middleware::from_fn_with_state,
    routing::{get, post},
};
use serde::Serialize;
use sqlx::{PgPool, Row};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{
    auth::{AuthMode, Authenticator},
    qr::QrSigner,
    sync::FixtureHrSource,
};

pub use state::{AppState, CurrentUser, UserDirectory};

pub fn router(
    pool: PgPool,
    auth_mode: AuthMode,
    authenticator: Box<dyn Authenticator>,
    signer: QrSigner,
    public_app_url: String,
) -> Router {
    let state = AppState::new(
        pool,
        auth_mode,
        authenticator,
        signer,
        public_app_url,
        std::sync::Arc::new(FixtureHrSource),
    );
    build_router(state)
}

pub fn build_router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/me", get(auth::me))
        .route("/attendance/today", get(attendance::today))
        .route("/attendance/scan", post(attendance::scan))
        .route("/attendance/scope", get(attendance::scoped))
        .route("/admin/qr", get(qr::current))
        .route("/admin/qr/{kind}/rotate", post(qr::rotate))
        .route("/admin/permissions", get(admin::permissions))
        .route("/admin/users", get(admin::users))
        .route(
            "/admin/permissions/admin-roles",
            post(admin::grant_admin_role),
        )
        .route(
            "/admin/permissions/admin-roles/{user_id}",
            axum::routing::delete(admin::revoke_admin_role),
        )
        .route("/admin/permissions/scopes", post(admin::grant_scope))
        .route(
            "/admin/permissions/scopes/{user_id}/{department_id}",
            axum::routing::delete(admin::revoke_scope),
        )
        .route("/admin/departments/tree", get(admin::departments))
        .route("/admin/audit", get(admin::audit))
        .route("/admin/hr-sync", get(sync::latest).post(sync::start))
        .route("/admin/hr-sync/runs/{run_id}", get(sync::details))
        .route_layer(from_fn_with_state(state.clone(), state::authenticate));

    Router::new()
        .route("/health", get(health))
        .route("/api/openapi.yaml", get(openapi))
        .route("/api/dev/users", get(dev_users))
        .nest("/api", protected)
        .layer(
            CorsLayer::new()
                .allow_origin(state.allowed_origin())
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
}
async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

async fn openapi() -> impl axum::response::IntoResponse {
    (
        [("content-type", "application/yaml; charset=utf-8")],
        include_str!("../../../docs/openapi.yaml"),
    )
}

#[derive(Serialize)]
struct DevUser {
    id: uuid::Uuid,
    display_name: String,
    email: Option<String>,
    employee_code: Option<String>,
}

async fn dev_users(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<Json<Vec<DevUser>>, crate::error::AppError> {
    if state.auth_mode != AuthMode::Development {
        return Err(crate::error::AppError::NotFound);
    }
    let rows = sqlx::query("SELECT id, display_name, email, employee_code FROM users WHERE employee_code LIKE 'DEMO-%' AND active = TRUE ORDER BY employee_code")
        .fetch_all(&state.pool).await.map_err(|_| crate::error::AppError::Internal)?;
    let users = rows
        .into_iter()
        .map(|row| {
            Ok(DevUser {
                id: row
                    .try_get("id")
                    .map_err(|_| crate::error::AppError::Internal)?,
                display_name: row
                    .try_get("display_name")
                    .map_err(|_| crate::error::AppError::Internal)?,
                email: row
                    .try_get("email")
                    .map_err(|_| crate::error::AppError::Internal)?,
                employee_code: row
                    .try_get("employee_code")
                    .map_err(|_| crate::error::AppError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>, crate::error::AppError>>()?;
    Ok(Json(users))
}
