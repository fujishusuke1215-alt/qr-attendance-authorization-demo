use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Serialize;
use uuid::Uuid;

use super::state::{AppState, CurrentUser, require_admin};
use crate::{
    error::AppError,
    sync::{SyncError, SyncRunDetails},
};

#[derive(Serialize)]
pub struct StartSyncResponse {
    pub run_id: Uuid,
    pub status: &'static str,
    pub poll_after_ms: u64,
}

pub async fn start(
    State(state): State<AppState>,
    Extension(actor): Extension<CurrentUser>,
) -> Result<(StatusCode, Json<StartSyncResponse>), AppError> {
    require_admin(&state, &actor).await?;
    let run_id = state.sync.start(actor.id).await.map_err(map_sync_error)?;
    let service = state.sync.clone();
    tokio::spawn(async move {
        if service.run(run_id).await.is_err() {
            tracing::error!(%run_id, "HR snapshot sync worker stopped after an internal error");
        }
    });
    Ok((
        StatusCode::ACCEPTED,
        Json(StartSyncResponse {
            run_id,
            status: "running",
            poll_after_ms: 750,
        }),
    ))
}

pub async fn latest(
    State(state): State<AppState>,
    Extension(actor): Extension<CurrentUser>,
) -> Result<Json<Option<SyncRunDetails>>, AppError> {
    require_admin(&state, &actor).await?;
    Ok(Json(state.sync.latest().await.map_err(map_sync_error)?))
}

pub async fn details(
    State(state): State<AppState>,
    Extension(actor): Extension<CurrentUser>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<SyncRunDetails>, AppError> {
    require_admin(&state, &actor).await?;
    state
        .sync
        .run_details(run_id)
        .await
        .map_err(map_sync_error)?
        .map(Json)
        .ok_or(AppError::NotFound)
}

fn map_sync_error(error: SyncError) -> AppError {
    match error {
        SyncError::Database => AppError::Internal,
    }
}
