use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Postgres, Row, Transaction, types::Json};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    authz::{Grant, GrantSource, reconcile_automatic_grants},
    hr::{HrSnapshot, snapshot_hash, validate_snapshot},
    org::{DepartmentInput, department_id},
};

const EMPLOYEE_NAMESPACE: Uuid = Uuid::from_u128(0xb145_45df_88c1_47d0_a631_7003_6c11_c0e1);
const HR_ADMIN_CODES: [&str; 2] = ["HR_ADMIN", "EXECUTIVE_SPONSOR"];
const MANAGER_CODES: [&str; 3] = ["DEPT_MANAGER", "AREA_MANAGER", "TEAM_LEAD"];
const STAGES: [&str; 5] = [
    "fetch",
    "validate",
    "departments",
    "people",
    "authorizations",
];

#[async_trait]
pub trait HrSource: Send + Sync {
    async fn load_snapshot(&self) -> Result<HrSnapshot, HrSourceError>;
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("external HR source is unavailable")]
pub struct HrSourceError;

#[derive(Clone, Default)]
pub struct FixtureHrSource;

#[async_trait]
impl HrSource for FixtureHrSource {
    async fn load_snapshot(&self) -> Result<HrSnapshot, HrSourceError> {
        serde_json::from_str(include_str!("../../fixtures/hr_snapshot.json"))
            .map_err(|_| HrSourceError)
    }
}

#[derive(Clone)]
pub struct SyncService {
    pool: PgPool,
    source: Arc<dyn HrSource>,
}

impl SyncService {
    pub fn new(pool: PgPool, source: Arc<dyn HrSource>) -> Self {
        Self { pool, source }
    }

    pub async fn start(&self, actor: Uuid) -> Result<Uuid, SyncError> {
        let run_id = Uuid::new_v4();
        sqlx::query("INSERT INTO hr_sync_runs(id, snapshot_hash, status, started_by) VALUES ($1, $2, 'running', $3)")
            .bind(run_id).bind(format!("pending:{run_id}")).bind(actor).execute(&self.pool).await.map_err(|_| SyncError::Database)?;
        for stage in STAGES {
            sqlx::query(
                "INSERT INTO hr_sync_stages(run_id, stage, status) VALUES ($1, $2, 'pending')",
            )
            .bind(run_id)
            .bind(stage)
            .execute(&self.pool)
            .await
            .map_err(|_| SyncError::Database)?;
        }
        Ok(run_id)
    }

    pub async fn run(&self, run_id: Uuid) -> Result<SyncResult, SyncError> {
        self.set_stage(run_id, "fetch", "running", None).await?;
        let snapshot = match self.source.load_snapshot().await {
            Ok(snapshot) => snapshot,
            Err(_) => {
                return self
                    .fail(
                        run_id,
                        "fetch",
                        "The HR snapshot source could not be read.",
                        0,
                    )
                    .await;
            }
        };
        self.set_stage(
            run_id,
            "fetch",
            "succeeded",
            Some(format!(
                "Loaded {} departments and {} employee records.",
                snapshot.departments.len(),
                snapshot.employees.len()
            )),
        )
        .await?;

        self.set_stage(run_id, "validate", "running", None).await?;
        if let Err(error) = validate_snapshot(&snapshot) {
            return self.fail(run_id, "validate", &error.to_string(), 0).await;
        }
        let fingerprint = snapshot_hash(&snapshot);
        sqlx::query("UPDATE hr_sync_runs SET snapshot_hash = $2 WHERE id = $1")
            .bind(run_id)
            .bind(&fingerprint)
            .execute(&self.pool)
            .await
            .map_err(|_| SyncError::Database)?;
        self.set_stage(
            run_id,
            "validate",
            "succeeded",
            Some(
                "Required fields, hierarchy, aliases, and employee affiliations are valid.".into(),
            ),
        )
        .await?;

        let prior = sqlx::query("SELECT id FROM hr_sync_runs WHERE snapshot_hash = $1 AND status = 'succeeded' AND id <> $2 ORDER BY completed_at DESC LIMIT 1")
            .bind(&fingerprint).bind(run_id).fetch_optional(&self.pool).await.map_err(|_| SyncError::Database)?;
        if let Some(row) = prior {
            for stage in ["departments", "people", "authorizations"] {
                self.set_stage(run_id, stage, "skipped", Some("The same validated snapshot was already applied; no rows or audit events were changed.".into())).await?;
            }
            sqlx::query("UPDATE hr_sync_runs SET status = 'succeeded', completed_at = now(), changed_records = 0, deduplicated = TRUE WHERE id = $1")
                .bind(run_id).execute(&self.pool).await.map_err(|_| SyncError::Database)?;
            let prior_run_id: Uuid = row.try_get("id").map_err(|_| SyncError::Database)?;
            return Ok(SyncResult {
                run_id,
                status: "succeeded".into(),
                changed_records: 0,
                deduplicated: true,
                previous_run_id: Some(prior_run_id),
            });
        }

        let mut changed = 0_i32;
        self.set_stage(run_id, "departments", "running", None)
            .await?;
        match reconcile_departments(&self.pool, &snapshot.departments).await {
            Ok(count) => {
                changed += count;
                self.set_stage(run_id, "departments", "succeeded", Some(format!("Applied department hierarchy and stable code mappings ({count} changed records)."))).await?;
            }
            Err(_) => {
                return self
                    .fail(
                        run_id,
                        "departments",
                        "Department hierarchy could not be saved; no later stages ran.",
                        changed,
                    )
                    .await;
            }
        }

        self.set_stage(run_id, "people", "running", None).await?;
        match reconcile_people(&self.pool, &snapshot).await {
            Ok(count) => {
                changed += count;
                self.set_stage(
                    run_id,
                    "people",
                    "succeeded",
                    Some(format!(
                        "Applied employee profiles and affiliations ({count} changed records)."
                    )),
                )
                .await?;
            }
            Err(_) => return self
                .fail(
                    run_id,
                    "people",
                    "Employee profiles or affiliations could not be saved; no later stages ran.",
                    changed,
                )
                .await,
        }

        self.set_stage(run_id, "authorizations", "running", None)
            .await?;
        match reconcile_authorizations(&self.pool, run_id, &snapshot).await {
            Ok(count) => { changed += count; self.set_stage(run_id, "authorizations", "succeeded", Some(format!("Reconciled HR-derived roles and department scopes ({count} changed grants)."))).await?; }
            Err(_) => return self.fail(run_id, "authorizations", "HR-derived permissions could not be reconciled; retrying the validated snapshot is safe.", changed).await,
        }

        sqlx::query("UPDATE hr_sync_runs SET status = 'succeeded', completed_at = now(), changed_records = $2 WHERE id = $1")
            .bind(run_id).bind(changed).execute(&self.pool).await.map_err(|_| SyncError::Database)?;
        if changed > 0 {
            sqlx::query("INSERT INTO audit_events(actor_user_id, action, subject_type, subject_id, detail) SELECT started_by, 'hr.snapshot.applied', 'hr_sync_run', id::text, $2 FROM hr_sync_runs WHERE id = $1")
                .bind(run_id).bind(Json(json!({"snapshot_hash": fingerprint, "changed_records": changed, "initiated_by": "manual_sync"})))
                .execute(&self.pool).await.map_err(|_| SyncError::Database)?;
        }
        Ok(SyncResult {
            run_id,
            status: "succeeded".into(),
            changed_records: changed,
            deduplicated: false,
            previous_run_id: None,
        })
    }

    async fn set_stage(
        &self,
        run_id: Uuid,
        stage: &str,
        status: &str,
        detail: Option<String>,
    ) -> Result<(), SyncError> {
        sqlx::query("UPDATE hr_sync_stages SET status = $3, started_at = CASE WHEN $3 = 'running' THEN COALESCE(started_at, now()) ELSE started_at END, completed_at = CASE WHEN $3 IN ('succeeded', 'failed', 'skipped') THEN now() ELSE completed_at END, detail = $4 WHERE run_id = $1 AND stage = $2")
            .bind(run_id).bind(stage).bind(status).bind(detail).execute(&self.pool).await.map_err(|_| SyncError::Database)?;
        Ok(())
    }

    async fn fail(
        &self,
        run_id: Uuid,
        failed_stage: &str,
        detail: &str,
        changed: i32,
    ) -> Result<SyncResult, SyncError> {
        self.set_stage(run_id, failed_stage, "failed", Some(detail.to_owned()))
            .await?;
        let stage_index = STAGES
            .iter()
            .position(|stage| *stage == failed_stage)
            .unwrap_or(0);
        for stage in STAGES.iter().skip(stage_index + 1) {
            self.set_stage(
                run_id,
                stage,
                "skipped",
                Some("Skipped because an earlier stage failed.".into()),
            )
            .await?;
        }
        sqlx::query("UPDATE hr_sync_runs SET status = 'failed', completed_at = now(), error_detail = $2, changed_records = $3 WHERE id = $1")
            .bind(run_id).bind(detail).bind(changed).execute(&self.pool).await.map_err(|_| SyncError::Database)?;
        Ok(SyncResult {
            run_id,
            status: "failed".into(),
            changed_records: changed,
            deduplicated: false,
            previous_run_id: None,
        })
    }

    pub async fn run_details(&self, run_id: Uuid) -> Result<Option<SyncRunDetails>, SyncError> {
        let Some(row) = sqlx::query("SELECT id, status, snapshot_hash, started_at, completed_at, error_detail, changed_records, deduplicated FROM hr_sync_runs WHERE id = $1")
            .bind(run_id).fetch_optional(&self.pool).await.map_err(|_| SyncError::Database)? else { return Ok(None); };
        let stages = sqlx::query("SELECT stage, status, started_at, completed_at, detail FROM hr_sync_stages WHERE run_id = $1 ORDER BY array_position(ARRAY['fetch','validate','departments','people','authorizations'], stage)")
            .bind(run_id).fetch_all(&self.pool).await.map_err(|_| SyncError::Database)?;
        let stage_results = stages
            .into_iter()
            .map(|stage| {
                Ok(RunStage {
                    name: stage.try_get("stage").map_err(|_| SyncError::Database)?,
                    status: stage.try_get("status").map_err(|_| SyncError::Database)?,
                    started_at: stage
                        .try_get("started_at")
                        .map_err(|_| SyncError::Database)?,
                    completed_at: stage
                        .try_get("completed_at")
                        .map_err(|_| SyncError::Database)?,
                    detail: stage.try_get("detail").map_err(|_| SyncError::Database)?,
                })
            })
            .collect::<Result<Vec<_>, SyncError>>()?;
        Ok(Some(SyncRunDetails {
            run_id: row.try_get("id").map_err(|_| SyncError::Database)?,
            status: row.try_get("status").map_err(|_| SyncError::Database)?,
            snapshot_hash: row
                .try_get("snapshot_hash")
                .map_err(|_| SyncError::Database)?,
            started_at: row.try_get("started_at").map_err(|_| SyncError::Database)?,
            completed_at: row
                .try_get("completed_at")
                .map_err(|_| SyncError::Database)?,
            error_detail: row
                .try_get("error_detail")
                .map_err(|_| SyncError::Database)?,
            changed_records: row
                .try_get("changed_records")
                .map_err(|_| SyncError::Database)?,
            deduplicated: row
                .try_get("deduplicated")
                .map_err(|_| SyncError::Database)?,
            stages: stage_results,
        }))
    }

    pub async fn latest(&self) -> Result<Option<SyncRunDetails>, SyncError> {
        let id = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM hr_sync_runs ORDER BY started_at DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| SyncError::Database)?;
        match id {
            Some(id) => self.run_details(id).await,
            None => Ok(None),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SyncResult {
    pub run_id: Uuid,
    pub status: String,
    pub changed_records: i32,
    pub deduplicated: bool,
    pub previous_run_id: Option<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SyncRunDetails {
    pub run_id: Uuid,
    pub status: String,
    pub snapshot_hash: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_detail: Option<String>,
    pub changed_records: i32,
    pub deduplicated: bool,
    pub stages: Vec<RunStage>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RunStage {
    pub name: String,
    pub status: String,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub detail: Option<String>,
}

pub type RunStageStatus = String;

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("database error")]
    Database,
}

async fn reconcile_departments(
    pool: &PgPool,
    departments: &[DepartmentInput],
) -> Result<i32, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let existing = sqlx::query("SELECT identity_code, id FROM departments")
        .fetch_all(&mut *transaction)
        .await?;
    let mut ids = existing
        .into_iter()
        .map(|row| {
            Ok((
                row.try_get::<String, _>("identity_code")?,
                row.try_get::<Uuid, _>("id")?,
            ))
        })
        .collect::<Result<HashMap<_, _>, sqlx::Error>>()?;
    for department in departments {
        ids.entry(department.identity_code.clone())
            .or_insert_with(|| department_id(&department.identity_code));
    }
    let mut pending = departments.to_vec();
    let mut changed = 0_i32;
    while !pending.is_empty() {
        let index = pending
            .iter()
            .position(|department| {
                department
                    .parent_identity_code
                    .as_ref()
                    .is_none_or(|parent| ids.contains_key(parent))
            })
            .ok_or(sqlx::Error::RowNotFound)?;
        let department = pending.remove(index);
        let parent_id = department
            .parent_identity_code
            .as_ref()
            .and_then(|parent| ids.get(parent))
            .copied();
        let id = ids[&department.identity_code];
        changed += sqlx::query("INSERT INTO departments(id, identity_code, name, parent_id, active) VALUES ($1, $2, $3, $4, $5) ON CONFLICT(identity_code) DO UPDATE SET name = EXCLUDED.name, parent_id = EXCLUDED.parent_id, active = EXCLUDED.active, updated_at = now() WHERE departments.name IS DISTINCT FROM EXCLUDED.name OR departments.parent_id IS DISTINCT FROM EXCLUDED.parent_id OR departments.active IS DISTINCT FROM EXCLUDED.active")
            .bind(id).bind(&department.identity_code).bind(&department.name).bind(parent_id).bind(department.active).execute(&mut *transaction).await?.rows_affected() as i32;
    }
    let identity_codes = departments
        .iter()
        .map(|department| department.identity_code.clone())
        .collect::<Vec<_>>();
    changed += sqlx::query("UPDATE departments SET active = FALSE, updated_at = now() WHERE active = TRUE AND identity_code <> ALL($1)")
        .bind(&identity_codes).execute(&mut *transaction).await?.rows_affected() as i32;

    let existing_codes =
        sqlx::query("SELECT code, department_id, is_primary FROM department_codes")
            .fetch_all(&mut *transaction)
            .await?;
    let current = existing_codes
        .into_iter()
        .map(|row| {
            Ok((
                row.try_get::<String, _>("code")?,
                (
                    row.try_get::<Uuid, _>("department_id")?,
                    row.try_get::<bool, _>("is_primary")?,
                ),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, sqlx::Error>>()?;
    let mut desired = BTreeMap::new();
    for department in departments {
        let department_uuid = ids[&department.identity_code];
        desired.insert(department.code.clone(), (department_uuid, true));
        for alias in &department.aliases {
            desired.insert(alias.clone(), (department_uuid, false));
        }
    }
    if current != desired {
        sqlx::query("DELETE FROM department_codes")
            .execute(&mut *transaction)
            .await?;
        for (code, (department_id, primary)) in &desired {
            sqlx::query(
                "INSERT INTO department_codes(code, department_id, is_primary) VALUES ($1, $2, $3)",
            )
            .bind(code)
            .bind(department_id)
            .bind(primary)
            .execute(&mut *transaction)
            .await?;
        }
        changed += current.len().max(desired.len()) as i32;
    }
    if changed > 0 {
        sqlx::query("INSERT INTO audit_events(action, subject_type, subject_id, detail) VALUES ('hr.departments.reconciled', 'department_snapshot', $1, $2)")
            .bind(department_id(&snapshot_name(departments)).to_string()).bind(Json(json!({"department_count": departments.len(), "changed_records": changed}))).execute(&mut *transaction).await?;
    }
    transaction.commit().await?;
    Ok(changed)
}

fn snapshot_name(departments: &[DepartmentInput]) -> String {
    departments
        .first()
        .map(|department| department.identity_code.as_str())
        .unwrap_or("empty")
        .to_owned()
}

async fn reconcile_people(pool: &PgPool, snapshot: &HrSnapshot) -> Result<i32, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let existing =
        sqlx::query("SELECT employee_code, id FROM users WHERE employee_code IS NOT NULL")
            .fetch_all(&mut *transaction)
            .await?;
    let mut ids = existing
        .into_iter()
        .map(|row| {
            Ok((
                row.try_get::<String, _>("employee_code")?,
                row.try_get::<Uuid, _>("id")?,
            ))
        })
        .collect::<Result<HashMap<_, _>, sqlx::Error>>()?;
    let mut changed = 0_i32;
    for employee in &snapshot.employees {
        let id = *ids
            .entry(employee.employee_code.clone())
            .or_insert_with(|| {
                Uuid::new_v5(&EMPLOYEE_NAMESPACE, employee.employee_code.as_bytes())
            });
        let email = employee
            .email
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        changed += sqlx::query("INSERT INTO users(id, employee_code, email, display_name, job_code, job_title, active) VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT(employee_code) DO UPDATE SET email = EXCLUDED.email, display_name = EXCLUDED.display_name, job_code = EXCLUDED.job_code, job_title = EXCLUDED.job_title, active = EXCLUDED.active, updated_at = now() WHERE users.email IS DISTINCT FROM EXCLUDED.email OR users.display_name IS DISTINCT FROM EXCLUDED.display_name OR users.job_code IS DISTINCT FROM EXCLUDED.job_code OR users.job_title IS DISTINCT FROM EXCLUDED.job_title OR users.active IS DISTINCT FROM EXCLUDED.active")
            .bind(id).bind(&employee.employee_code).bind(email).bind(employee.display_name.trim()).bind(employee.job_code.as_deref()).bind(employee.job_title.as_deref()).bind(employee.active).execute(&mut *transaction).await?.rows_affected() as i32;
    }
    let codes = snapshot
        .employees
        .iter()
        .map(|employee| employee.employee_code.clone())
        .collect::<Vec<_>>();
    changed += sqlx::query("UPDATE users SET active = FALSE, updated_at = now() WHERE active = TRUE AND employee_code IS NOT NULL AND employee_code <> ALL($1)")
        .bind(&codes).execute(&mut *transaction).await?.rows_affected() as i32;

    let dept_rows = sqlx::query("SELECT identity_code, id FROM departments")
        .fetch_all(&mut *transaction)
        .await?;
    let departments = dept_rows
        .into_iter()
        .map(|row| {
            Ok((
                row.try_get::<String, _>("identity_code")?,
                row.try_get::<Uuid, _>("id")?,
            ))
        })
        .collect::<Result<HashMap<_, _>, sqlx::Error>>()?;
    let incoming = snapshot
        .employees
        .iter()
        .flat_map(|employee| {
            employee
                .department_identity_codes
                .iter()
                .map(|identity| (ids[&employee.employee_code], departments[identity]))
        })
        .collect::<BTreeSet<_>>();
    let current_rows = sqlx::query("SELECT employee_departments.user_id, employee_departments.department_id FROM employee_departments JOIN users ON users.id = employee_departments.user_id WHERE users.employee_code IS NOT NULL")
        .fetch_all(&mut *transaction).await?;
    let current = current_rows
        .into_iter()
        .map(|row| {
            Ok((
                row.try_get::<Uuid, _>("user_id")?,
                row.try_get::<Uuid, _>("department_id")?,
            ))
        })
        .collect::<Result<BTreeSet<_>, sqlx::Error>>()?;
    if current != incoming {
        sqlx::query("DELETE FROM employee_departments USING users WHERE employee_departments.user_id = users.id AND users.employee_code IS NOT NULL").execute(&mut *transaction).await?;
        for (user_id, department_id) in &incoming {
            sqlx::query("INSERT INTO employee_departments(user_id, department_id) VALUES ($1, $2)")
                .bind(user_id)
                .bind(department_id)
                .execute(&mut *transaction)
                .await?;
        }
        changed += current.symmetric_difference(&incoming).count() as i32;
    }
    if changed > 0 {
        sqlx::query("INSERT INTO audit_events(action, subject_type, subject_id, detail) VALUES ('hr.people.reconciled', 'employee_snapshot', $1, $2)")
            .bind(snapshot.snapshot_id.as_str()).bind(Json(json!({"employee_count": snapshot.employees.len(), "changed_records": changed}))).execute(&mut *transaction).await?;
    }
    transaction.commit().await?;
    Ok(changed)
}

async fn reconcile_authorizations(
    pool: &PgPool,
    run_id: Uuid,
    snapshot: &HrSnapshot,
) -> Result<i32, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let user_rows = sqlx::query("SELECT id, employee_code, job_code FROM users WHERE employee_code IS NOT NULL AND active = TRUE").fetch_all(&mut *transaction).await?;
    let users = user_rows
        .into_iter()
        .map(|row| {
            Ok((
                row.try_get::<String, _>("employee_code")?,
                (
                    row.try_get::<Uuid, _>("id")?,
                    row.try_get::<Option<String>, _>("job_code")?,
                ),
            ))
        })
        .collect::<Result<HashMap<_, _>, sqlx::Error>>()?;
    let department_rows =
        sqlx::query("SELECT identity_code, id FROM departments WHERE active = TRUE")
            .fetch_all(&mut *transaction)
            .await?;
    let departments = department_rows
        .into_iter()
        .map(|row| {
            Ok((
                row.try_get::<String, _>("identity_code")?,
                row.try_get::<Uuid, _>("id")?,
            ))
        })
        .collect::<Result<HashMap<_, _>, sqlx::Error>>()?;
    let mut desired_scopes = BTreeSet::new();
    let mut desired_admins = BTreeSet::new();
    for employee in &snapshot.employees {
        if !employee.active {
            continue;
        }
        let Some((user_id, role_code)) = users.get(&employee.employee_code) else {
            continue;
        };
        if role_code
            .as_deref()
            .is_some_and(|code| HR_ADMIN_CODES.contains(&code))
        {
            desired_admins.insert(*user_id);
        }
        if role_code
            .as_deref()
            .is_some_and(|code| MANAGER_CODES.contains(&code))
        {
            for identity in &employee.department_identity_codes {
                if let Some(department_id) = departments.get(identity) {
                    desired_scopes.insert((*user_id, *department_id));
                }
            }
        }
    }

    let current_scope_rows = sqlx::query(
        "SELECT user_id, department_id FROM manager_scopes WHERE grant_source = 'external_hr_role'",
    )
    .fetch_all(&mut *transaction)
    .await?;
    let current_scopes = current_scope_rows
        .into_iter()
        .map(|row| {
            Ok(Grant {
                user_id: row.try_get("user_id")?,
                department_id: row.try_get("department_id")?,
                source: GrantSource::ExternalHrRole,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let scope_delta = reconcile_automatic_grants(&current_scopes, &desired_scopes);
    let mut changed = 0_i32;
    for grant in scope_delta.revoke {
        let result = sqlx::query("DELETE FROM manager_scopes WHERE user_id = $1 AND department_id = $2 AND grant_source = 'external_hr_role'")
            .bind(grant.user_id).bind(grant.department_id).execute(&mut *transaction).await?;
        if result.rows_affected() > 0 {
            changed += 1;
            append_permission_audit(
                &mut transaction,
                run_id,
                "authorization.hr_scope_revoked",
                grant.user_id,
                grant.department_id,
            )
            .await?;
        }
    }
    for grant in scope_delta.grant {
        let result = sqlx::query("INSERT INTO manager_scopes(user_id, department_id, grant_source) VALUES ($1, $2, 'external_hr_role') ON CONFLICT DO NOTHING")
            .bind(grant.user_id).bind(grant.department_id).execute(&mut *transaction).await?;
        if result.rows_affected() > 0 {
            changed += 1;
            append_permission_audit(
                &mut transaction,
                run_id,
                "authorization.hr_scope_granted",
                grant.user_id,
                grant.department_id,
            )
            .await?;
        }
    }

    let current_admin_rows = sqlx::query(
        "SELECT user_id FROM user_roles WHERE role = 'admin' AND grant_source = 'external_hr_role'",
    )
    .fetch_all(&mut *transaction)
    .await?;
    let current_admins = current_admin_rows
        .into_iter()
        .map(|row| row.try_get::<Uuid, _>("user_id"))
        .collect::<Result<BTreeSet<_>, _>>()?;
    for user_id in current_admins.difference(&desired_admins) {
        let result = sqlx::query("DELETE FROM user_roles WHERE user_id = $1 AND role = 'admin' AND grant_source = 'external_hr_role'").bind(user_id).execute(&mut *transaction).await?;
        changed += result.rows_affected() as i32;
        if result.rows_affected() > 0 {
            sqlx::query("INSERT INTO audit_events(actor_user_id, action, subject_type, subject_id, detail) VALUES (NULL, 'authorization.hr_admin_revoked', 'user', $1, $2)")
                .bind(user_id.to_string()).bind(Json(json!({"sync_run_id": run_id}))).execute(&mut *transaction).await?;
        }
    }
    for user_id in desired_admins.difference(&current_admins) {
        let result = sqlx::query("INSERT INTO user_roles(user_id, role, grant_source) VALUES ($1, 'admin', 'external_hr_role') ON CONFLICT DO NOTHING").bind(user_id).execute(&mut *transaction).await?;
        changed += result.rows_affected() as i32;
        if result.rows_affected() > 0 {
            sqlx::query("INSERT INTO audit_events(actor_user_id, action, subject_type, subject_id, detail) VALUES (NULL, 'authorization.hr_admin_granted', 'user', $1, $2)")
                .bind(user_id.to_string()).bind(Json(json!({"sync_run_id": run_id}))).execute(&mut *transaction).await?;
        }
    }
    transaction.commit().await?;
    Ok(changed)
}

async fn append_permission_audit(
    transaction: &mut Transaction<'_, Postgres>,
    run_id: Uuid,
    action: &str,
    user_id: Uuid,
    department_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO audit_events(actor_user_id, action, subject_type, subject_id, detail) VALUES (NULL, $1, 'manager_scope', $2, $3)")
        .bind(action).bind(format!("{user_id}:{department_id}")).bind(Json(json!({"sync_run_id": run_id, "grant_source": "external_hr_role"}))).execute(&mut **transaction).await?;
    Ok(())
}
