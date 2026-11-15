CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE users (
    id UUID PRIMARY KEY,
    entra_oid TEXT UNIQUE,
    employee_code TEXT UNIQUE,
    email TEXT UNIQUE,
    display_name TEXT NOT NULL,
    job_code TEXT,
    job_title TEXT,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE departments (
    id UUID PRIMARY KEY,
    identity_code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    parent_id UUID REFERENCES departments(id) ON DELETE RESTRICT,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE department_codes (
    code TEXT PRIMARY KEY,
    department_id UUID NOT NULL REFERENCES departments(id) ON DELETE CASCADE,
    is_primary BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE INDEX department_codes_department_idx ON department_codes(department_id);

CREATE TABLE employee_departments (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    department_id UUID NOT NULL REFERENCES departments(id) ON DELETE RESTRICT,
    PRIMARY KEY (user_id, department_id)
);

CREATE TABLE user_roles (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('admin')),
    grant_source TEXT NOT NULL CHECK (grant_source IN ('manual', 'external_hr_role')),
    granted_by UUID REFERENCES users(id) ON DELETE SET NULL,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, role, grant_source)
);

CREATE TABLE manager_scopes (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    department_id UUID NOT NULL REFERENCES departments(id) ON DELETE CASCADE,
    grant_source TEXT NOT NULL CHECK (grant_source IN ('manual', 'external_hr_role')),
    granted_by UUID REFERENCES users(id) ON DELETE SET NULL,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, department_id, grant_source)
);
CREATE INDEX manager_scopes_by_user_idx ON manager_scopes(user_id);

CREATE TABLE qr_channels (
    kind TEXT PRIMARY KEY CHECK (kind IN ('check_in', 'check_out')),
    revision BIGINT NOT NULL CHECK (revision > 0),
    active BOOLEAN NOT NULL DEFAULT TRUE,
    updated_by UUID REFERENCES users(id) ON DELETE SET NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
INSERT INTO qr_channels(kind, revision) VALUES ('check_in', 1), ('check_out', 1);
CREATE TABLE qr_revision_history (
    kind TEXT NOT NULL CHECK (kind IN ('check_in', 'check_out')),
    revision BIGINT NOT NULL CHECK (revision > 0),
    issued_by UUID REFERENCES users(id) ON DELETE SET NULL,
    issued_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY(kind, revision)
);
INSERT INTO qr_revision_history(kind, revision) VALUES ('check_in', 1), ('check_out', 1);

CREATE TABLE attendance_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    kind TEXT NOT NULL CHECK (kind IN ('check_in', 'check_out')),
    work_date DATE NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    qr_revision BIGINT NOT NULL,
    idempotency_key UUID NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, work_date, kind)
);
CREATE INDEX attendance_events_by_date_idx ON attendance_events(work_date, kind);
CREATE INDEX attendance_events_by_user_idx ON attendance_events(user_id, work_date DESC);

CREATE TABLE audit_events (
    id BIGSERIAL PRIMARY KEY,
    actor_user_id UUID REFERENCES users(id) ON DELETE RESTRICT,
    action TEXT NOT NULL,
    subject_type TEXT,
    subject_id TEXT,
    detail JSONB NOT NULL DEFAULT '{}'::jsonb,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX audit_events_recent_idx ON audit_events(occurred_at DESC);

CREATE FUNCTION prevent_audit_mutation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'audit_events is append-only';
END;
$$;
CREATE TRIGGER audit_events_append_only
    BEFORE UPDATE OR DELETE ON audit_events
    FOR EACH ROW EXECUTE FUNCTION prevent_audit_mutation();

CREATE TABLE hr_sync_runs (
    id UUID PRIMARY KEY,
    snapshot_hash TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed')),
    started_by UUID REFERENCES users(id) ON DELETE SET NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    error_detail TEXT,
    changed_records INTEGER NOT NULL DEFAULT 0,
    deduplicated BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE TABLE hr_sync_stages (
    run_id UUID NOT NULL REFERENCES hr_sync_runs(id) ON DELETE CASCADE,
    stage TEXT NOT NULL CHECK (stage IN ('fetch', 'validate', 'departments', 'people', 'authorizations')),
    status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'succeeded', 'failed', 'skipped')),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    detail TEXT,
    PRIMARY KEY (run_id, stage)
);

-- Fictional local identities. No real directory or employee data is included.
INSERT INTO users(id, employee_code, email, display_name, job_code, job_title) VALUES
    ('10000000-0000-4000-8000-000000000001', 'DEMO-ADMIN', 'admin@example.test', 'Aoi Admin', 'HR_ADMIN', 'Demo administrator'),
    ('10000000-0000-4000-8000-000000000002', 'DEMO-MANAGER', 'manager@example.test', 'Ren Manager', 'DEPT_MANAGER', 'Demo department manager'),
    ('10000000-0000-4000-8000-000000000003', 'DEMO-STAFF', 'staff@example.test', 'Mio Staff', 'ASSOCIATE', 'Demo employee');

INSERT INTO departments(id, identity_code, name, parent_id) VALUES
    ('20000000-0000-4000-8000-000000000001', 'ORG-ROOT', 'Example Holdings', NULL),
    ('20000000-0000-4000-8000-000000000002', 'ORG-ENGINEERING', 'Engineering', '20000000-0000-4000-8000-000000000001'),
    ('20000000-0000-4000-8000-000000000003', 'ORG-PEOPLE', 'People Operations', '20000000-0000-4000-8000-000000000001');

INSERT INTO department_codes(code, department_id, is_primary) VALUES
    ('E-000', '20000000-0000-4000-8000-000000000001', TRUE),
    ('E-110', '20000000-0000-4000-8000-000000000002', TRUE),
    ('P-210', '20000000-0000-4000-8000-000000000003', TRUE);

INSERT INTO employee_departments(user_id, department_id) VALUES
    ('10000000-0000-4000-8000-000000000002', '20000000-0000-4000-8000-000000000002'),
    ('10000000-0000-4000-8000-000000000003', '20000000-0000-4000-8000-000000000002'),
    ('10000000-0000-4000-8000-000000000001', '20000000-0000-4000-8000-000000000003');

INSERT INTO user_roles(user_id, role, grant_source, granted_by)
VALUES ('10000000-0000-4000-8000-000000000001', 'admin', 'manual', '10000000-0000-4000-8000-000000000001');
INSERT INTO manager_scopes(user_id, department_id, grant_source)
VALUES ('10000000-0000-4000-8000-000000000002', '20000000-0000-4000-8000-000000000002', 'external_hr_role');
