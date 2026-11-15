use std::env;

use attendance_service::{
    api,
    auth::{AuthMode, DevelopmentAuthenticator},
    hr::HrSnapshot,
    qr::{QrKind, QrSigner},
    sync::{FixtureHrSource, HrSource, HrSourceError, SyncService},
};
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

struct StaticSource(HrSnapshot);

#[async_trait::async_trait]
impl HrSource for StaticSource {
    async fn load_snapshot(&self) -> Result<HrSnapshot, HrSourceError> {
        Ok(self.0.clone())
    }
}

#[tokio::test]
async fn postgres_acceptance_covers_auth_attendance_qr_permissions_and_hr_reconciliation() {
    let Ok(database_url) = env::var("TEST_DATABASE_URL").or_else(|_| env::var("DATABASE_URL"))
    else {
        eprintln!(
            "TEST_DATABASE_URL/DATABASE_URL is unset; PostgreSQL acceptance checks require the CI database service."
        );
        return;
    };
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&database_url)
        .await
        .expect("PostgreSQL test database must be reachable");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrations should apply cleanly");

    let complete_user_id = insert_test_user(&pool, Some("present@example.test")).await;
    let incomplete_user_id = insert_test_user(&pool, None).await;
    let signer = QrSigner::from_base64(
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        "head-office",
    )
    .unwrap();
    let app = api::router(
        pool.clone(),
        AuthMode::Development,
        Box::new(DevelopmentAuthenticator),
        signer.clone(),
        "http://localhost:5173".into(),
    );

    let denied = app
        .clone()
        .oneshot(request(
            "GET",
            "/api/admin/departments/tree",
            complete_user_id,
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(
        denied.status(),
        StatusCode::FORBIDDEN,
        "a signed-in employee without an admin grant cannot call a protected admin API"
    );

    let revision: i64 =
        sqlx::query_scalar("SELECT revision FROM qr_channels WHERE kind = 'check_in'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let token = signer.derive(QrKind::CheckIn, revision);
    let key = Uuid::new_v4();
    let first = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/attendance/scan",
            complete_user_id,
            Some(key),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);
    let replay = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/attendance/scan",
            complete_user_id,
            Some(key),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(
        replay.status(),
        StatusCode::OK,
        "retry with the same idempotency key returns the original event"
    );
    let duplicate_workday = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/attendance/scan",
            complete_user_id,
            Some(Uuid::new_v4()),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(
        duplicate_workday.status(),
        StatusCode::CONFLICT,
        "a second check-in with a different key on the same Tokyo workday is rejected"
    );
    let record_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM attendance_events WHERE user_id = $1")
            .bind(complete_user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(record_count, 1);

    let profile_failure = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/attendance/scan",
            incomplete_user_id,
            Some(Uuid::new_v4()),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(profile_failure.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let missing_auth = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/attendance/scan")
                .header("idempotency-key", Uuid::new_v4().to_string())
                .header("content-type", "application/json")
                .body(Body::from(json!({"token": token}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_auth.status(), StatusCode::UNAUTHORIZED);
    let complete_count_after_missing_auth: i64 =
        sqlx::query_scalar("SELECT count(*) FROM attendance_events WHERE user_id = $1")
            .bind(complete_user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        complete_count_after_missing_auth, 1,
        "an unauthenticated scan cannot add an attendance event"
    );
    let incomplete_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM attendance_events WHERE user_id = $1")
            .bind(incomplete_user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        incomplete_count, 0,
        "missing email/profile data cannot create an attendance event"
    );

    sqlx::query(
        "INSERT INTO user_roles(user_id, role, grant_source) VALUES ($1, 'admin', 'manual')",
    )
    .bind(complete_user_id)
    .execute(&pool)
    .await
    .unwrap();
    let department_id = Uuid::parse_str("20000000-0000-4000-8000-000000000002").unwrap();
    let grant = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/admin/permissions/scopes",
            complete_user_id,
            json!({"user_id": incomplete_user_id, "department_id": department_id}),
        ))
        .await
        .unwrap();
    assert_eq!(
        grant.status(),
        StatusCode::OK,
        "authorized administrator can create a manual scope"
    );
    let revoke = app
        .clone()
        .oneshot(request(
            "DELETE",
            &format!("/api/admin/permissions/scopes/{incomplete_user_id}/{department_id}"),
            complete_user_id,
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(revoke.status(), StatusCode::OK);
    let manual_audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE actor_user_id = $1 AND action IN ('authorization.manual_scope_granted', 'authorization.manual_scope_revoked')").bind(complete_user_id).fetch_one(&pool).await.unwrap();
    assert_eq!(
        manual_audit_count, 2,
        "manual grant and revocation are both audited"
    );

    let rotate = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/admin/qr/check_in/rotate",
            complete_user_id,
            json!({"expected_revision": revision}),
        ))
        .await
        .unwrap();
    assert_eq!(rotate.status(), StatusCode::OK);
    let rotated: Value =
        serde_json::from_slice(&to_bytes(rotate.into_body(), usize::MAX).await.unwrap()).unwrap();
    let rotated_revision = rotated["channel"]["revision"].as_i64().unwrap();
    let rotated_token = rotated["channel"]["token"].as_str().unwrap();
    assert_eq!(rotated_revision, revision + 1);
    let stale_rotation = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/admin/qr/check_in/rotate",
            complete_user_id,
            json!({"expected_revision": revision}),
        ))
        .await
        .unwrap();
    assert_eq!(
        stale_rotation.status(),
        StatusCode::CONFLICT,
        "a stale revision cannot overwrite a newer rotation"
    );
    let old_token_scan = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/attendance/scan",
            complete_user_id,
            Some(Uuid::new_v4()),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(
        old_token_scan.status(),
        StatusCode::BAD_REQUEST,
        "rotation immediately invalidates the old QR"
    );
    let new_token_scan = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/attendance/scan",
            complete_user_id,
            Some(Uuid::new_v4()),
            Some(rotated_token),
        ))
        .await
        .unwrap();
    assert_eq!(
        new_token_scan.status(),
        StatusCode::CONFLICT,
        "the rotated QR verifies, then the explicit one-check-in-per-day rule rejects a duplicate"
    );

    let admin_id = Uuid::parse_str("10000000-0000-4000-8000-000000000001").unwrap();
    let base_source = FixtureHrSource;
    let base_snapshot = base_source.load_snapshot().await.unwrap();
    let sync = SyncService::new(pool.clone(), std::sync::Arc::new(base_source));
    let first_run = sync.start(admin_id).await.unwrap();
    assert_eq!(sync.run(first_run).await.unwrap().status, "succeeded");
    let first_details = sync.run_details(first_run).await.unwrap().unwrap();
    assert!(
        first_details
            .stages
            .iter()
            .all(|stage| stage.status == "succeeded"),
        "each successful synchronization phase is observable"
    );
    let audit_count_before_repeat: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    let repeat_run = sync.start(admin_id).await.unwrap();
    let repeat_result = sync.run(repeat_run).await.unwrap();
    assert!(
        repeat_result.deduplicated,
        "the same complete snapshot is a no-op"
    );
    let audit_count_after_repeat: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        audit_count_after_repeat, audit_count_before_repeat,
        "an identical snapshot does not append redundant audit events"
    );

    let manager_id = Uuid::parse_str("10000000-0000-4000-8000-000000000002").unwrap();
    let engineering_id = Uuid::parse_str("20000000-0000-4000-8000-000000000002").unwrap();
    sqlx::query("INSERT INTO manager_scopes(user_id, department_id, grant_source, granted_by) VALUES ($1, $2, 'manual', $3) ON CONFLICT DO NOTHING")
        .bind(manager_id).bind(engineering_id).bind(admin_id).execute(&pool).await.unwrap();
    let mut promoted = base_snapshot.clone();
    promoted.snapshot_id = format!("{}-promotion-{}", promoted.snapshot_id, Uuid::new_v4());
    promoted
        .employees
        .iter_mut()
        .find(|person| person.employee_code == "DEMO-MANAGER")
        .unwrap()
        .job_code = Some("ASSOCIATE".into());
    let promotion = SyncService::new(pool.clone(), std::sync::Arc::new(StaticSource(promoted)));
    let promotion_run = promotion.start(admin_id).await.unwrap();
    assert_eq!(
        promotion.run(promotion_run).await.unwrap().status,
        "succeeded"
    );
    let manual_remains: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM manager_scopes WHERE user_id = $1 AND department_id = $2 AND grant_source = 'manual')").bind(manager_id).bind(engineering_id).fetch_one(&pool).await.unwrap();
    let automatic_remains: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM manager_scopes WHERE user_id = $1 AND department_id = $2 AND grant_source = 'external_hr_role')").bind(manager_id).bind(engineering_id).fetch_one(&pool).await.unwrap();
    assert!(
        manual_remains,
        "promotion reconciliation must not modify the independent manual row"
    );
    assert!(
        !automatic_remains,
        "only the now-unneeded HR-derived row is removed"
    );
    sqlx::query("DELETE FROM manager_scopes WHERE user_id = $1 AND department_id = $2 AND grant_source = 'manual'").bind(manager_id).bind(engineering_id).execute(&pool).await.unwrap();

    let restore = SyncService::new(
        pool.clone(),
        std::sync::Arc::new(StaticSource(base_snapshot.clone())),
    );
    let restore_run = restore.start(admin_id).await.unwrap();
    assert_eq!(restore.run(restore_run).await.unwrap().status, "succeeded");
    let scopes_before_bad_snapshot: i64 = sqlx::query_scalar("SELECT count(*) FROM manager_scopes")
        .fetch_one(&pool)
        .await
        .unwrap();
    let roles_before_bad_snapshot: i64 = sqlx::query_scalar("SELECT count(*) FROM user_roles")
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut malformed = base_snapshot;
    malformed.snapshot_id = format!("malformed-{}", Uuid::new_v4());
    malformed.employees[0].job_code_field_present = false;
    let invalid_sync = SyncService::new(pool.clone(), std::sync::Arc::new(StaticSource(malformed)));
    let invalid_run = invalid_sync.start(admin_id).await.unwrap();
    assert_eq!(
        invalid_sync.run(invalid_run).await.unwrap().status,
        "failed"
    );
    let scopes_after_bad_snapshot: i64 = sqlx::query_scalar("SELECT count(*) FROM manager_scopes")
        .fetch_one(&pool)
        .await
        .unwrap();
    let roles_after_bad_snapshot: i64 = sqlx::query_scalar("SELECT count(*) FROM user_roles")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        scopes_before_bad_snapshot, scopes_after_bad_snapshot,
        "missing job_code_field cannot mass-revoke scopes"
    );
    assert_eq!(
        roles_before_bad_snapshot, roles_after_bad_snapshot,
        "missing job_code_field cannot mass-revoke role grants"
    );
    let failed_details = invalid_sync
        .run_details(invalid_run)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        failed_details
            .stages
            .iter()
            .find(|stage| stage.name == "validate")
            .unwrap()
            .status,
        "failed"
    );
    assert!(
        failed_details
            .stages
            .iter()
            .filter(
                |stage| ["departments", "people", "authorizations"].contains(&stage.name.as_str())
            )
            .all(|stage| stage.status == "skipped")
    );

    cleanup_test_user(&pool, complete_user_id).await;
    cleanup_test_user(&pool, incomplete_user_id).await;
}

fn request(
    method: &str,
    path: &str,
    user_id: Uuid,
    idempotency_key: Option<Uuid>,
    token: Option<&str>,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("x-dev-user", user_id.to_string());
    if let Some(key) = idempotency_key {
        builder = builder.header("idempotency-key", key.to_string());
    }
    if token.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    builder
        .body(Body::from(
            token
                .map(|token| json!({"token": token}).to_string())
                .unwrap_or_default(),
        ))
        .unwrap()
}

fn json_request(method: &str, path: &str, user_id: Uuid, payload: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("x-dev-user", user_id.to_string())
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap()
}

async fn insert_test_user(pool: &PgPool, email: Option<&str>) -> Uuid {
    let id = Uuid::new_v4();
    let employee_code = format!("ACCEPTANCE-{id}");
    sqlx::query("INSERT INTO users(id, employee_code, email, display_name, job_code, job_title) VALUES ($1, $2, $3, $4, 'ASSOCIATE', 'Acceptance test')")
        .bind(id).bind(employee_code).bind(email).bind("Acceptance Test").execute(pool).await.unwrap();
    id
}

async fn cleanup_test_user(pool: &PgPool, id: Uuid) {
    sqlx::query("DELETE FROM attendance_events WHERE user_id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}
