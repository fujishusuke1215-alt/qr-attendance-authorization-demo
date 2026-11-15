use std::process;

use attendance_service::{api, auth::build_authenticator, config::Config};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env().unwrap_or_else(|message| {
        tracing::error!(%message, "invalid service configuration");
        process::exit(2);
    });
    let pool = PgPoolOptions::new()
        .max_connections(16)
        .connect(&config.database_url)
        .await
        .unwrap_or_else(|_| {
            tracing::error!("could not connect to the configured database");
            process::exit(2);
        });
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .unwrap_or_else(|_| {
            tracing::error!("database migrations could not be applied");
            process::exit(2);
        });
    let authenticator = build_authenticator(
        config.auth_mode,
        config.entra_tenant_id,
        config.entra_api_audience,
    )
    .unwrap_or_else(|message| {
        tracing::error!(%message, "authentication adapter configuration is invalid");
        process::exit(2);
    });
    let app = api::router(
        pool,
        config.auth_mode,
        authenticator,
        config.qr_signer,
        config.public_app_url,
    );
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .unwrap_or_else(|_| {
            tracing::error!("could not bind the configured API address");
            process::exit(2);
        });
    tracing::info!(address = %config.listen, environment = %config.app_env, "attendance service listening");
    if axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await
        .is_err()
    {
        tracing::error!("HTTP server stopped unexpectedly");
        process::exit(1);
    }
}

#[cfg(unix)]
async fn shutdown() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}

#[cfg(not(unix))]
async fn shutdown() {
    tokio::signal::ctrl_c().await.ok();
}
