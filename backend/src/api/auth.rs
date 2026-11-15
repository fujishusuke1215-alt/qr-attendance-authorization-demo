use axum::{Extension, Json, extract::State};
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use super::state::{AppState, CurrentUser, is_admin};
use crate::error::AppError;

#[derive(Serialize)]
pub struct CurrentSession {
    pub user: CurrentUser,
    pub capabilities: Capabilities,
}

#[derive(Serialize)]
pub struct Capabilities {
    pub admin: bool,
    pub manager_department_ids: Vec<Uuid>,
}

pub async fn me(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<CurrentSession>, AppError> {
    let admin = is_admin(&state, &user).await?;
    let rows = sqlx::query("SELECT DISTINCT department_id FROM manager_scopes WHERE user_id = $1 ORDER BY department_id")
        .bind(user.id).fetch_all(&state.pool).await.map_err(|_| AppError::Internal)?;
    let manager_department_ids = rows
        .into_iter()
        .map(|row| row.try_get("department_id").map_err(|_| AppError::Internal))
        .collect::<Result<Vec<Uuid>, AppError>>()?;
    Ok(Json(CurrentSession {
        user,
        capabilities: Capabilities {
            admin,
            manager_department_ids,
        },
    }))
}
