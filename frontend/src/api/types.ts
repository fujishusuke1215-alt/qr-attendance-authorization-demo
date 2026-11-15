export type QrKind = "check_in" | "check_out";
export type GrantSource = "manual" | "external_hr_role";
export type StageStatus = "pending" | "running" | "succeeded" | "failed" | "skipped";

export interface UserProfile {
  id: string;
  display_name: string;
  email: string | null;
  employee_code: string | null;
  job_code: string | null;
}

export interface CurrentSession {
  user: UserProfile;
  capabilities: { admin: boolean; manager_department_ids: string[] };
}

export interface DemoUser extends Pick<UserProfile, "id" | "display_name" | "email" | "employee_code"> {}
export type UserOption = DemoUser;

export interface AttendanceRecord {
  id: string;
  user_id: string;
  display_name: string;
  email: string | null;
  department_name: string | null;
  kind: QrKind;
  work_date: string;
  occurred_at: string;
  qr_revision: number;
}

export interface TodayResponse {
  work_date: string;
  check_in: AttendanceRecord | null;
  check_out: AttendanceRecord | null;
}

export interface QrChannel {
  kind: QrKind;
  revision: number;
  active: boolean;
  token: string | null;
  scan_url: string | null;
  issued_at: string;
  issued_by: string | null;
}

export interface QrRevision {
  kind: QrKind;
  revision: number;
  issued_at: string;
  issued_by: string | null;
}

export interface QrState { channels: QrChannel[]; history: QrRevision[] }
export interface Department {
  id: string;
  identity_code: string;
  name: string;
  parent_id: string | null;
  active: boolean;
  member_count: number;
}

export interface AuthorizationGrant {
  user_id: string;
  display_name: string;
  email: string | null;
  permission: "admin" | "manager_scope";
  department_id: string | null;
  department_name: string | null;
  grant_source: GrantSource;
  granted_by: string | null;
  granted_at: string;
}

export interface SyncStage {
  name: string;
  status: StageStatus;
  started_at: string | null;
  completed_at: string | null;
  detail: string | null;
}

export interface SyncRun {
  run_id: string;
  status: "running" | "succeeded" | "failed";
  snapshot_hash: string;
  started_at: string;
  completed_at: string | null;
  error_detail: string | null;
  changed_records: number;
  deduplicated: boolean;
  stages: SyncStage[];
}

export interface AuditEntry {
  id: number;
  actor_user_id: string | null;
  action: string;
  subject_type: string | null;
  subject_id: string | null;
  detail: Record<string, unknown>;
  occurred_at: string;
}
