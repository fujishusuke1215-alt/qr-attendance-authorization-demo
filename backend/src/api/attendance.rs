use axum::{
    Extension, Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use super::state::{AppState, CurrentUser, is_admin};
use crate::{
    attendance::{AttendanceRecord, work_date_in_tokyo},
    error::AppError,
    qr::QrKind,
};

#[derive(Serialize)]
pub struct TodayResponse {
    pub work_date: NaiveDate,
    pub check_in: Option<AttendanceRecord>,
    pub check_out: Option<AttendanceRecord>,
}

pub async fn today(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<TodayResponse>, AppError> {
    ensure_complete_profile(&user)?;
    let date = work_date_in_tokyo(Utc::now());
    let rows = sqlx::query("SELECT e.id, e.user_id, u.display_name, u.email, e.kind, e.work_date, e.occurred_at, e.qr_revision, (SELECT string_agg(d.name, ' / ' ORDER BY d.name) FROM employee_departments ed JOIN departments d ON d.id = ed.department_id WHERE ed.user_id = e.user_id) AS department_name FROM attendance_events e JOIN users u ON u.id = e.user_id WHERE e.user_id = $1 AND e.work_date = $2 ORDER BY e.occurred_at")
        .bind(user.id).bind(date).fetch_all(&state.pool).await.map_err(|_| AppError::Internal)?;
    let mut response = TodayResponse {
        work_date: date,
        check_in: None,
        check_out: None,
    };
    for row in rows {
        let record = attendance_record(&row)?;
        match record.kind {
            QrKind::CheckIn => response.check_in = Some(record),
            QrKind::CheckOut => response.check_out = Some(record),
        }
    }
    Ok(Json(response))
}

#[derive(Deserialize)]
pub struct ScanRequest {
    pub token: String,
}

#[derive(Serialize)]
pub struct ScanResponse {
    pub created: bool,
    pub event: AttendanceRecord,
}

pub async fn scan(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    headers: HeaderMap,
    Json(request): Json<ScanRequest>,
) -> Result<(StatusCode, Json<ScanResponse>), AppError> {
    ensure_complete_profile(&user)?;
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or(AppError::InvalidInput("Idempotency-Key must be a UUID"))?;
    let rows = sqlx::query("SELECT kind, revision, active FROM qr_channels")
        .fetch_all(&state.pool)
        .await
        .map_err(|_| AppError::Internal)?;
    let mut verified = None;
    for row in rows {
        let kind_name: String = row.try_get("kind").map_err(|_| AppError::Internal)?;
        let kind = match kind_name.as_str() {
            "check_in" => QrKind::CheckIn,
            "check_out" => QrKind::CheckOut,
            _ => continue,
        };
        let revision: i64 = row.try_get("revision").map_err(|_| AppError::Internal)?;
        let active: bool = row.try_get("active").map_err(|_| AppError::Internal)?;
        if active && state.signer.verify(&request.token, kind, revision) {
            verified = Some((kind, revision));
            break;
        }
    }
    let (kind, revision) = verified.ok_or(AppError::InvalidInput(
        "QR token is invalid, inactive, or has been replaced",
    ))?;
    let work_date = work_date_in_tokyo(Utc::now());
    let occurred_at = Utc::now();
    let mut transaction = state.pool.begin().await.map_err(|_| AppError::Internal)?;
    let replay = sqlx::query("SELECT id, user_id, kind, work_date, occurred_at, qr_revision FROM attendance_events WHERE idempotency_key = $1")
        .bind(idempotency_key).fetch_optional(&mut *transaction).await.map_err(|_| AppError::Internal)?;
    if let Some(row) = replay {
        let replay_user: Uuid = row.try_get("user_id").map_err(|_| AppError::Internal)?;
        let replay_kind: String = row.try_get("kind").map_err(|_| AppError::Internal)?;
        if replay_user != user.id || replay_kind != kind.as_str() {
            return Err(AppError::Conflict(
                "idempotency key has already been used for a different scan",
            ));
        }
        transaction.commit().await.map_err(|_| AppError::Internal)?;
        let event = load_event(
            &state,
            user.id,
            row.try_get("id").map_err(|_| AppError::Internal)?,
        )
        .await?;
        return Ok((
            StatusCode::OK,
            Json(ScanResponse {
                created: false,
                event,
            }),
        ));
    }

    let inserted = sqlx::query("INSERT INTO attendance_events(user_id, kind, work_date, occurred_at, qr_revision, idempotency_key) VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT DO NOTHING RETURNING id")
        .bind(user.id).bind(kind.as_str()).bind(work_date).bind(occurred_at).bind(revision).bind(idempotency_key).fetch_optional(&mut *transaction).await.map_err(|_| AppError::Internal)?;
    let Some(row) = inserted else {
        return Err(AppError::Conflict(
            "one event of this kind is already recorded for this workday",
        ));
    };
    let event_id: Uuid = row.try_get("id").map_err(|_| AppError::Internal)?;
    transaction.commit().await.map_err(|_| AppError::Internal)?;
    let event = load_event(&state, user.id, event_id).await?;
    Ok((
        StatusCode::CREATED,
        Json(ScanResponse {
            created: true,
            event,
        }),
    ))
}

pub async fn scoped(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<ScopeQuery>,
) -> Result<Json<Vec<AttendanceRecord>>, AppError> {
    let date = query.date.unwrap_or_else(|| work_date_in_tokyo(Utc::now()));
    let admin = is_admin(&state, &user).await?;
    let scope_rows = sqlx::query("SELECT s.department_id FROM manager_scopes s JOIN departments d ON d.id = s.department_id WHERE s.user_id = $1 AND d.active = TRUE")
        .bind(user.id).fetch_all(&state.pool).await.map_err(|_| AppError::Internal)?;
    let scopes = scope_rows
        .into_iter()
        .map(|row| {
            row.try_get::<Uuid, _>("department_id")
                .map_err(|_| AppError::Internal)
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    if !admin && scopes.is_empty() {
        return Err(AppError::Forbidden);
    }
    let rows = sqlx::query("SELECT e.id, e.user_id, u.display_name, u.email, e.kind, e.work_date, e.occurred_at, e.qr_revision, (SELECT string_agg(d.name, ' / ' ORDER BY d.name) FROM employee_departments ed JOIN departments d ON d.id = ed.department_id WHERE ed.user_id = e.user_id AND ($2 OR d.id = ANY($3))) AS department_name FROM attendance_events e JOIN users u ON u.id = e.user_id WHERE e.work_date = $1 AND ($2 OR EXISTS(SELECT 1 FROM employee_departments ed WHERE ed.user_id = e.user_id AND ed.department_id = ANY($3))) ORDER BY e.occurred_at DESC")
        .bind(date).bind(admin).bind(&scopes).fetch_all(&state.pool).await.map_err(|_| AppError::Internal)?;
    rows.iter()
        .map(attendance_record)
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

#[derive(Deserialize)]
pub struct ScopeQuery {
    pub date: Option<NaiveDate>,
}

fn ensure_complete_profile(user: &CurrentUser) -> Result<(), AppError> {
    if user.has_complete_attendance_profile() {
        Ok(())
    } else {
        Err(AppError::IncompleteProfile)
    }
}

fn attendance_record(row: &sqlx::postgres::PgRow) -> Result<AttendanceRecord, AppError> {
    let kind = match row
        .try_get::<String, _>("kind")
        .map_err(|_| AppError::Internal)?
        .as_str()
    {
        "check_in" => QrKind::CheckIn,
        "check_out" => QrKind::CheckOut,
        _ => return Err(AppError::Internal),
    };
    Ok(AttendanceRecord {
        id: row.try_get("id").map_err(|_| AppError::Internal)?,
        user_id: row.try_get("user_id").map_err(|_| AppError::Internal)?,
        display_name: row
            .try_get("display_name")
            .map_err(|_| AppError::Internal)?,
        email: row.try_get("email").map_err(|_| AppError::Internal)?,
        department_name: row
            .try_get("department_name")
            .map_err(|_| AppError::Internal)?,
        kind,
        work_date: row.try_get("work_date").map_err(|_| AppError::Internal)?,
        occurred_at: row.try_get("occurred_at").map_err(|_| AppError::Internal)?,
        qr_revision: row.try_get("qr_revision").map_err(|_| AppError::Internal)?,
    })
}

async fn load_event(
    state: &AppState,
    user_id: Uuid,
    event_id: Uuid,
) -> Result<AttendanceRecord, AppError> {
    let row = sqlx::query("SELECT e.id, e.user_id, u.display_name, u.email, e.kind, e.work_date, e.occurred_at, e.qr_revision, (SELECT string_agg(d.name, ' / ' ORDER BY d.name) FROM employee_departments ed JOIN departments d ON d.id = ed.department_id WHERE ed.user_id = e.user_id) AS department_name FROM attendance_events e JOIN users u ON u.id = e.user_id WHERE e.id = $1 AND e.user_id = $2")
        .bind(event_id).bind(user_id).fetch_one(&state.pool).await.map_err(|_| AppError::Internal)?;
    attendance_record(&row)
}

#[cfg(test)]
mod tests {
    use super::ensure_complete_profile;
    use crate::{api::CurrentUser, error::AppError};
    use chrono::{DateTime, Utc};

    #[test]
    fn scan_requires_complete_authenticated_profile() {
        let user = CurrentUser {
            id: uuid::Uuid::nil(),
            display_name: "".into(),
            email: None,
            employee_code: None,
            job_code: None,
        };
        assert!(matches!(
            ensure_complete_profile(&user),
            Err(AppError::IncompleteProfile)
        ));
    }

    #[test]
    fn scan_records_have_a_server_generated_timestamp() {
        let time = DateTime::parse_from_rfc3339("2026-11-15T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(time.to_rfc3339(), "2026-11-15T00:00:00+00:00");
    }
}
