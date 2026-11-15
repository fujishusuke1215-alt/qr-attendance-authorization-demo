use axum::{
    Extension, Json,
    extract::{Path, State},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use super::state::{AppState, CurrentUser, require_admin};
use crate::{error::AppError, qr::QrKind};

#[derive(Clone, Debug, Serialize)]
pub struct QrChannelView {
    pub kind: QrKind,
    pub revision: i64,
    pub active: bool,
    pub token: Option<String>,
    pub scan_url: Option<String>,
    pub issued_at: DateTime<Utc>,
    pub issued_by: Option<Uuid>,
}

#[derive(Clone, Debug, Serialize)]
pub struct QrRevisionView {
    pub kind: QrKind,
    pub revision: i64,
    pub issued_at: DateTime<Utc>,
    pub issued_by: Option<Uuid>,
}

#[derive(Serialize)]
pub struct QrState {
    pub channels: Vec<QrChannelView>,
    pub history: Vec<QrRevisionView>,
}

pub async fn current(
    State(state): State<AppState>,
    Extension(actor): Extension<CurrentUser>,
) -> Result<Json<QrState>, AppError> {
    require_admin(&state, &actor).await?;
    let rows = sqlx::query("SELECT c.kind, c.revision, c.active, h.issued_at, h.issued_by FROM qr_channels c JOIN qr_revision_history h ON h.kind = c.kind AND h.revision = c.revision ORDER BY c.kind")
        .fetch_all(&state.pool).await.map_err(|_| AppError::Internal)?;
    let mut channels = Vec::new();
    for row in rows {
        let kind = parse_kind(
            row.try_get::<String, _>("kind")
                .map_err(|_| AppError::Internal)?
                .as_str(),
        )?;
        let revision: i64 = row.try_get("revision").map_err(|_| AppError::Internal)?;
        let active: bool = row.try_get("active").map_err(|_| AppError::Internal)?;
        let token = active.then(|| state.signer.derive(kind, revision));
        let scan_url = token.as_ref().map(|token| {
            format!(
                "{}/scan/{token}",
                state.public_app_url.trim_end_matches('/')
            )
        });
        channels.push(QrChannelView {
            kind,
            revision,
            active,
            token,
            scan_url,
            issued_at: row.try_get("issued_at").map_err(|_| AppError::Internal)?,
            issued_by: row.try_get("issued_by").map_err(|_| AppError::Internal)?,
        });
    }
    let history_rows = sqlx::query("SELECT kind, revision, issued_at, issued_by FROM qr_revision_history ORDER BY issued_at DESC LIMIT 50")
        .fetch_all(&state.pool).await.map_err(|_| AppError::Internal)?;
    let history = history_rows
        .into_iter()
        .map(|row| {
            Ok(QrRevisionView {
                kind: parse_kind(
                    row.try_get::<String, _>("kind")
                        .map_err(|_| AppError::Internal)?
                        .as_str(),
                )?,
                revision: row.try_get("revision").map_err(|_| AppError::Internal)?,
                issued_at: row.try_get("issued_at").map_err(|_| AppError::Internal)?,
                issued_by: row.try_get("issued_by").map_err(|_| AppError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(Json(QrState { channels, history }))
}

#[derive(Deserialize)]
pub struct RotateRequest {
    pub expected_revision: i64,
}

#[derive(Serialize)]
pub struct RotateResponse {
    pub channel: QrChannelView,
}

pub async fn rotate(
    State(state): State<AppState>,
    Extension(actor): Extension<CurrentUser>,
    Path(kind_path): Path<String>,
    Json(request): Json<RotateRequest>,
) -> Result<Json<RotateResponse>, AppError> {
    require_admin(&state, &actor).await?;
    let kind = parse_kind(&kind_path)?;
    if request.expected_revision < 0 {
        return Err(AppError::InvalidInput("revision cannot be negative"));
    }
    let mut transaction = state.pool.begin().await.map_err(|_| AppError::Internal)?;
    let row = sqlx::query("SELECT revision FROM qr_channels WHERE kind = $1 FOR UPDATE")
        .bind(kind.as_str())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NotFound)?;
    let current_revision: i64 = row.try_get("revision").map_err(|_| AppError::Internal)?;
    if current_revision != request.expected_revision {
        return Err(AppError::Conflict("QR revision changed"));
    }
    let new_revision = current_revision.checked_add(1).ok_or(AppError::Internal)?;
    sqlx::query("UPDATE qr_channels SET revision = $2, active = TRUE, updated_by = $3, updated_at = now() WHERE kind = $1")
        .bind(kind.as_str()).bind(new_revision).bind(actor.id).execute(&mut *transaction).await.map_err(|_| AppError::Internal)?;
    let history = sqlx::query("INSERT INTO qr_revision_history(kind, revision, issued_by) VALUES ($1, $2, $3) RETURNING issued_at")
        .bind(kind.as_str()).bind(new_revision).bind(actor.id).fetch_one(&mut *transaction).await.map_err(|_| AppError::Internal)?;
    sqlx::query("INSERT INTO audit_events(actor_user_id, action, subject_type, subject_id, detail) VALUES ($1, 'qr.revision.rotated', 'qr_channel', $2, $3)")
        .bind(actor.id).bind(kind.as_str()).bind(sqlx::types::Json(json!({"previous_revision":current_revision,"revision":new_revision,"old_qr_invalidated":true})))
        .execute(&mut *transaction).await.map_err(|_| AppError::Internal)?;
    transaction.commit().await.map_err(|_| AppError::Internal)?;
    let token = state.signer.derive(kind, new_revision);
    let scan_url = format!(
        "{}/scan/{token}",
        state.public_app_url.trim_end_matches('/')
    );
    Ok(Json(RotateResponse {
        channel: QrChannelView {
            kind,
            revision: new_revision,
            active: true,
            token: Some(token),
            scan_url: Some(scan_url),
            issued_at: history
                .try_get("issued_at")
                .map_err(|_| AppError::Internal)?,
            issued_by: Some(actor.id),
        },
    }))
}

fn parse_kind(value: &str) -> Result<QrKind, AppError> {
    match value {
        "check_in" => Ok(QrKind::CheckIn),
        "check_out" => Ok(QrKind::CheckOut),
        _ => Err(AppError::InvalidInput("unknown QR action")),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_kind;
    use crate::qr::QrKind;

    #[test]
    fn qr_action_path_is_explicit() {
        assert_eq!(parse_kind("check_in").unwrap(), QrKind::CheckIn);
        assert_eq!(parse_kind("check_out").unwrap(), QrKind::CheckOut);
        assert!(parse_kind("attendance").is_err());
    }
}
