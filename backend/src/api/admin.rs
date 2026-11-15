use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Row, types::Json as SqlJson};
use uuid::Uuid;

use super::state::{AppState, CurrentUser, require_admin};
use crate::error::AppError;

#[derive(Clone, Debug, Serialize)]
pub struct AuthorizationGrant {
    pub user_id: Uuid,
    pub display_name: String,
    pub email: Option<String>,
    pub permission: String,
    pub department_id: Option<Uuid>,
    pub department_name: Option<String>,
    pub grant_source: String,
    pub granted_by: Option<Uuid>,
    pub granted_at: DateTime<Utc>,
}

pub async fn permissions(
    State(state): State<AppState>,
    Extension(actor): Extension<CurrentUser>,
) -> Result<Json<Vec<AuthorizationGrant>>, AppError> {
    require_admin(&state, &actor).await?;
    let rows = sqlx::query(
        "SELECT u.id AS user_id, u.display_name, u.email, 'admin'::text AS permission, NULL::uuid AS department_id, NULL::text AS department_name, r.grant_source, r.granted_by, r.granted_at FROM user_roles r JOIN users u ON u.id = r.user_id WHERE r.role = 'admin' \
         UNION ALL \
         SELECT u.id AS user_id, u.display_name, u.email, 'manager_scope'::text AS permission, d.id AS department_id, d.name AS department_name, s.grant_source, s.granted_by, s.granted_at FROM manager_scopes s JOIN users u ON u.id = s.user_id JOIN departments d ON d.id = s.department_id \
         ORDER BY display_name, permission, department_name")
        .fetch_all(&state.pool).await.map_err(|_| AppError::Internal)?;
    let result = rows
        .into_iter()
        .map(|row| {
            Ok(AuthorizationGrant {
                user_id: row.try_get("user_id").map_err(|_| AppError::Internal)?,
                display_name: row
                    .try_get("display_name")
                    .map_err(|_| AppError::Internal)?,
                email: row.try_get("email").map_err(|_| AppError::Internal)?,
                permission: row.try_get("permission").map_err(|_| AppError::Internal)?,
                department_id: row
                    .try_get("department_id")
                    .map_err(|_| AppError::Internal)?,
                department_name: row
                    .try_get("department_name")
                    .map_err(|_| AppError::Internal)?,
                grant_source: row
                    .try_get("grant_source")
                    .map_err(|_| AppError::Internal)?,
                granted_by: row.try_get("granted_by").map_err(|_| AppError::Internal)?,
                granted_at: row.try_get("granted_at").map_err(|_| AppError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(Json(result))
}

#[derive(Clone, Debug, Serialize)]
pub struct UserOption {
    pub id: Uuid,
    pub display_name: String,
    pub email: Option<String>,
    pub employee_code: Option<String>,
}

pub async fn users(
    State(state): State<AppState>,
    Extension(actor): Extension<CurrentUser>,
) -> Result<Json<Vec<UserOption>>, AppError> {
    require_admin(&state, &actor).await?;
    let rows = sqlx::query("SELECT id, display_name, email, employee_code FROM users WHERE active = TRUE ORDER BY display_name")
        .fetch_all(&state.pool).await.map_err(|_| AppError::Internal)?;
    let options = rows
        .into_iter()
        .map(|row| {
            Ok(UserOption {
                id: row.try_get("id").map_err(|_| AppError::Internal)?,
                display_name: row
                    .try_get("display_name")
                    .map_err(|_| AppError::Internal)?,
                email: row.try_get("email").map_err(|_| AppError::Internal)?,
                employee_code: row
                    .try_get("employee_code")
                    .map_err(|_| AppError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(Json(options))
}

#[derive(Deserialize)]
pub struct UserGrantRequest {
    pub user_id: Uuid,
}

pub async fn grant_admin_role(
    State(state): State<AppState>,
    Extension(actor): Extension<CurrentUser>,
    Json(request): Json<UserGrantRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &actor).await?;
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM users WHERE id = $1 AND active = TRUE)",
    )
    .bind(request.user_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::Internal)?;
    if !exists {
        return Err(AppError::NotFound);
    }
    let mut transaction = state.pool.begin().await.map_err(|_| AppError::Internal)?;
    let result = sqlx::query("INSERT INTO user_roles(user_id, role, grant_source, granted_by) VALUES ($1, 'admin', 'manual', $2) ON CONFLICT DO NOTHING")
        .bind(request.user_id).bind(actor.id).execute(&mut *transaction).await.map_err(|_| AppError::Internal)?;
    if result.rows_affected() > 0 {
        insert_audit(
            &mut transaction,
            actor.id,
            "authorization.manual_admin_granted",
            "user",
            request.user_id.to_string(),
            json!({}),
        )
        .await?;
    }
    transaction.commit().await.map_err(|_| AppError::Internal)?;
    Ok(Json(
        json!({"updated": result.rows_affected() > 0, "effective": true}),
    ))
}

pub async fn revoke_admin_role(
    State(state): State<AppState>,
    Extension(actor): Extension<CurrentUser>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &actor).await?;
    if user_id == actor.id {
        return Err(AppError::Conflict("cannot revoke the acting administrator"));
    }
    let mut transaction = state.pool.begin().await.map_err(|_| AppError::Internal)?;
    let result = sqlx::query(
        "DELETE FROM user_roles WHERE user_id = $1 AND role = 'admin' AND grant_source = 'manual'",
    )
    .bind(user_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| AppError::Internal)?;
    if result.rows_affected() > 0 {
        insert_audit(
            &mut transaction,
            actor.id,
            "authorization.manual_admin_revoked",
            "user",
            user_id.to_string(),
            json!({}),
        )
        .await?;
    }
    let still_admin = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM user_roles WHERE user_id = $1 AND role = 'admin')",
    )
    .bind(user_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| AppError::Internal)?;
    transaction.commit().await.map_err(|_| AppError::Internal)?;
    Ok(Json(
        json!({"updated": result.rows_affected() > 0, "effective": still_admin}),
    ))
}

#[derive(Deserialize)]
pub struct ScopeGrantRequest {
    pub user_id: Uuid,
    pub department_id: Uuid,
}

pub async fn grant_scope(
    State(state): State<AppState>,
    Extension(actor): Extension<CurrentUser>,
    Json(request): Json<ScopeGrantRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &actor).await?;
    let exists = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1 AND active = TRUE) AND EXISTS(SELECT 1 FROM departments WHERE id = $2 AND active = TRUE)")
        .bind(request.user_id).bind(request.department_id).fetch_one(&state.pool).await.map_err(|_| AppError::Internal)?;
    if !exists {
        return Err(AppError::NotFound);
    }
    let mut transaction = state.pool.begin().await.map_err(|_| AppError::Internal)?;
    let result = sqlx::query("INSERT INTO manager_scopes(user_id, department_id, grant_source, granted_by) VALUES ($1, $2, 'manual', $3) ON CONFLICT DO NOTHING")
        .bind(request.user_id).bind(request.department_id).bind(actor.id).execute(&mut *transaction).await.map_err(|_| AppError::Internal)?;
    if result.rows_affected() > 0 {
        insert_audit(
            &mut transaction,
            actor.id,
            "authorization.manual_scope_granted",
            "manager_scope",
            format!("{}:{}", request.user_id, request.department_id),
            json!({"grant_source":"manual"}),
        )
        .await?;
    }
    transaction.commit().await.map_err(|_| AppError::Internal)?;
    Ok(Json(
        json!({"updated": result.rows_affected() > 0, "grant_source":"manual"}),
    ))
}

pub async fn revoke_scope(
    State(state): State<AppState>,
    Extension(actor): Extension<CurrentUser>,
    Path((user_id, department_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &actor).await?;
    let mut transaction = state.pool.begin().await.map_err(|_| AppError::Internal)?;
    let result = sqlx::query("DELETE FROM manager_scopes WHERE user_id = $1 AND department_id = $2 AND grant_source = 'manual'")
        .bind(user_id).bind(department_id).execute(&mut *transaction).await.map_err(|_| AppError::Internal)?;
    if result.rows_affected() > 0 {
        insert_audit(
            &mut transaction,
            actor.id,
            "authorization.manual_scope_revoked",
            "manager_scope",
            format!("{user_id}:{department_id}"),
            json!({"grant_source":"manual"}),
        )
        .await?;
    }
    let automatic_remains = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM manager_scopes WHERE user_id = $1 AND department_id = $2 AND grant_source = 'external_hr_role')")
        .bind(user_id).bind(department_id).fetch_one(&mut *transaction).await.map_err(|_| AppError::Internal)?;
    transaction.commit().await.map_err(|_| AppError::Internal)?;
    Ok(Json(
        json!({"updated": result.rows_affected() > 0, "automatic_grant_remains": automatic_remains}),
    ))
}

#[derive(Serialize)]
pub struct DepartmentRow {
    pub id: Uuid,
    pub identity_code: String,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub active: bool,
    pub member_count: i64,
}

pub async fn departments(
    State(state): State<AppState>,
    Extension(actor): Extension<CurrentUser>,
) -> Result<Json<Vec<DepartmentRow>>, AppError> {
    require_admin(&state, &actor).await?;
    let rows = sqlx::query("SELECT d.id, d.identity_code, d.name, d.parent_id, d.active, count(DISTINCT u.id)::bigint AS member_count FROM departments d LEFT JOIN employee_departments ed ON ed.department_id = d.id LEFT JOIN users u ON u.id = ed.user_id AND u.active = TRUE GROUP BY d.id ORDER BY d.name")
        .fetch_all(&state.pool).await.map_err(|_| AppError::Internal)?;
    let result = rows
        .into_iter()
        .map(|row| {
            Ok(DepartmentRow {
                id: row.try_get("id").map_err(|_| AppError::Internal)?,
                identity_code: row
                    .try_get("identity_code")
                    .map_err(|_| AppError::Internal)?,
                name: row.try_get("name").map_err(|_| AppError::Internal)?,
                parent_id: row.try_get("parent_id").map_err(|_| AppError::Internal)?,
                active: row.try_get("active").map_err(|_| AppError::Internal)?,
                member_count: row
                    .try_get("member_count")
                    .map_err(|_| AppError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(Json(result))
}

#[derive(Deserialize)]
pub struct AuditQuery {
    pub limit: Option<i64>,
}
#[derive(Serialize)]
pub struct AuditRow {
    pub id: i64,
    pub actor_user_id: Option<Uuid>,
    pub action: String,
    pub subject_type: Option<String>,
    pub subject_id: Option<String>,
    pub detail: Value,
    pub occurred_at: DateTime<Utc>,
}

pub async fn audit(
    State(state): State<AppState>,
    Extension(actor): Extension<CurrentUser>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditRow>>, AppError> {
    require_admin(&state, &actor).await?;
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let rows = sqlx::query("SELECT id, actor_user_id, action, subject_type, subject_id, detail, occurred_at FROM audit_events ORDER BY occurred_at DESC, id DESC LIMIT $1").bind(limit).fetch_all(&state.pool).await.map_err(|_| AppError::Internal)?;
    let result = rows
        .into_iter()
        .map(|row| {
            Ok(AuditRow {
                id: row.try_get("id").map_err(|_| AppError::Internal)?,
                actor_user_id: row
                    .try_get("actor_user_id")
                    .map_err(|_| AppError::Internal)?,
                action: row.try_get("action").map_err(|_| AppError::Internal)?,
                subject_type: row
                    .try_get("subject_type")
                    .map_err(|_| AppError::Internal)?,
                subject_id: row.try_get("subject_id").map_err(|_| AppError::Internal)?,
                detail: row
                    .try_get::<SqlJson<Value>, _>("detail")
                    .map_err(|_| AppError::Internal)?
                    .0,
                occurred_at: row.try_get("occurred_at").map_err(|_| AppError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(Json(result))
}

async fn insert_audit(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: Uuid,
    action: &str,
    subject_type: &str,
    subject_id: String,
    detail: Value,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO audit_events(actor_user_id, action, subject_type, subject_id, detail) VALUES ($1, $2, $3, $4, $5)")
        .bind(actor).bind(action).bind(subject_type).bind(subject_id).bind(SqlJson(detail)).execute(&mut **transaction).await.map_err(|_| AppError::Internal)?;
    Ok(())
}
