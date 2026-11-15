import { apiRequest, jsonBody } from "./client";
import type { AuditEntry, AuthorizationGrant, Department, QrKind, QrState, SyncRun, UserOption } from "./types";

export const getQrState = (): Promise<QrState> => apiRequest("/admin/qr");
export const rotateQr = (kind: QrKind, expected_revision: number) =>
  apiRequest<{ channel: QrState["channels"][number] }>(`/admin/qr/${kind}/rotate`, { method: "POST", body: jsonBody({ expected_revision }) });

export const getPermissions = (): Promise<AuthorizationGrant[]> => apiRequest("/admin/permissions");
export const getUsers = (): Promise<UserOption[]> => apiRequest("/admin/users");
export const getDepartments = (): Promise<Department[]> => apiRequest("/admin/departments/tree");
export const grantAdminRole = (user_id: string) => apiRequest("/admin/permissions/admin-roles", { method: "POST", body: jsonBody({ user_id }) });
export const revokeAdminRole = (user_id: string) => apiRequest(`/admin/permissions/admin-roles/${user_id}`, { method: "DELETE" });
export const grantScope = (user_id: string, department_id: string) => apiRequest("/admin/permissions/scopes", { method: "POST", body: jsonBody({ user_id, department_id }) });
export const revokeScope = (user_id: string, department_id: string) => apiRequest(`/admin/permissions/scopes/${user_id}/${department_id}`, { method: "DELETE" });
export const getAudit = (): Promise<AuditEntry[]> => apiRequest("/admin/audit?limit=50");

export const getLatestSync = (): Promise<SyncRun | null> => apiRequest("/admin/hr-sync");
export const startSync = (): Promise<{ run_id: string; status: string; poll_after_ms: number }> => apiRequest("/admin/hr-sync", { method: "POST" });
export const getSyncRun = (runId: string): Promise<SyncRun> => apiRequest(`/admin/hr-sync/runs/${runId}`);
