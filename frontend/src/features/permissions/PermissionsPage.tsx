import { useCallback, useEffect, useState } from "react";
import { getAudit, getDepartments, getPermissions, getUsers, grantAdminRole, grantScope, revokeAdminRole, revokeScope } from "../../api/adminApi";
import { ApiError } from "../../api/client";
import type { AuditEntry, AuthorizationGrant, Department, UserOption } from "../../api/types";
import { DepartmentTree } from "./DepartmentTree";
import { GrantSourceBadge } from "./GrantSourceBadge";

export function PermissionsPage() {
  const [grants, setGrants] = useState<AuthorizationGrant[]>([]);
  const [departments, setDepartments] = useState<Department[]>([]);
  const [users, setUsers] = useState<UserOption[]>([]);
  const [audit, setAudit] = useState<AuditEntry[]>([]);
  const [targetUser, setTargetUser] = useState("");
  const [selectedDepartmentIds, setSelectedDepartmentIds] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [nextGrants, nextDepartments, nextUsers, nextAudit] = await Promise.all([getPermissions(), getDepartments(), getUsers(), getAudit()]);
      setGrants(nextGrants);
      setDepartments(nextDepartments);
      setUsers(nextUsers);
      setAudit(nextAudit);
      setTargetUser((current) => current || nextUsers[0]?.id || "");
    } catch (cause) {
      setError(cause instanceof ApiError && cause.status === 403 ? "管理者権限がありません。" : cause instanceof Error ? cause.message : "権限データを取得できませんでした。");
    } finally { setLoading(false); }
  }, []);

  useEffect(() => { void load(); }, [load]);

  async function perform(operation: () => Promise<unknown>, successMessage: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await operation();
      await load();
      setNotice(successMessage);
    } catch (cause) {
      setError(cause instanceof ApiError && cause.status === 403 ? "管理者権限がないため変更できません。" : cause instanceof Error ? cause.message : "権限を更新できませんでした。");
    } finally { setBusy(false); }
  }

  async function grantSelectedScopes() {
    if (!targetUser || selectedDepartmentIds.length === 0) return;
    await perform(async () => {
      for (const departmentId of selectedDepartmentIds) await grantScope(targetUser, departmentId);
    }, `${selectedDepartmentIds.length} 件の手動閲覧範囲を付与しました。`);
    setSelectedDepartmentIds([]);
  }

  const target = users.find((user) => user.id === targetUser);

  return <div className="page">
    <header className="page-title"><p className="eyebrow">ACCESS CONTROL</p><h1>権限管理</h1><p>HRの役職コードから算出する自動権限と、管理者が付与する手動権限を個別に確認・操作できます。</p></header>
    {error && <div className="alert alert-error" role="alert">{error}</div>}
    {notice && <div className="alert alert-success" role="status">{notice}</div>}

    <div className="permission-layout">
      <section className="surface grant-editor">
        <div className="section-heading"><div><p className="eyebrow">MANUAL GRANT</p><h2>手動で権限を付与</h2></div><span className="icon-badge">＋</span></div>
        <label className="field-label" htmlFor="grant-user">対象社員</label>
        <select id="grant-user" className="select-control" value={targetUser} onChange={(event) => setTargetUser(event.target.value)} disabled={loading || users.length === 0}>
          <option value="">社員を選択</option>{users.map((user) => <option value={user.id} key={user.id}>{user.display_name}{user.employee_code ? ` · ${user.employee_code}` : ""}</option>)}
        </select>
        <div className="admin-grant-box"><div><strong>システム管理者</strong><p>すべての部門と設定を管理できます。</p></div><button className="button button-secondary" type="button" disabled={!targetUser || busy} onClick={() => void perform(() => grantAdminRole(targetUser), "管理者権限を手動付与しました。")}>管理者を付与</button></div>
        <div className="tree-section-heading"><div><strong>部門の閲覧範囲</strong><p>部門を選ぶと、その配下の有効な部門も選択されます。</p></div><span className="count-chip">{selectedDepartmentIds.length} 選択</span></div>
        <div className="tree-box"><DepartmentTree departments={departments} selectedIds={selectedDepartmentIds} onChange={setSelectedDepartmentIds} /></div>
        <button className="button button-primary button-wide" type="button" disabled={!targetUser || selectedDepartmentIds.length === 0 || busy} onClick={() => void grantSelectedScopes()}>{busy ? "更新中…" : "選択部門への閲覧権限を付与"}</button>
        {target && <p className="editor-caption">付与先: {target.display_name} <span>・</span> 変更内容は監査履歴に記録されます。</p>}
      </section>

      <section className="surface grants-panel">
        <div className="section-heading"><div><p className="eyebrow">EFFECTIVE ACCESS</p><h2>現在の付与一覧</h2></div><span className="count-chip">{grants.length} 件</span></div>
        {loading ? <div className="loading-card"><span className="spinner" />権限を読み込んでいます</div> : grants.length === 0 ? <div className="empty-state">権限付与はありません。</div> : <div className="grant-list">
          {grants.map((grant) => <GrantRow key={`${grant.user_id}:${grant.permission}:${grant.department_id ?? "all"}:${grant.grant_source}`} grant={grant} onRevoke={() => void perform(() => grant.permission === "admin" ? revokeAdminRole(grant.user_id) : revokeScope(grant.user_id, grant.department_id!), grant.permission === "admin" ? "手動管理者権限を取り消しました。" : "手動閲覧権限を取り消しました。")} busy={busy} />)}
        </div>}
        <div className="grant-legend"><span><GrantSourceBadge source="manual" /> 管理者の操作</span><span><GrantSourceBadge source="external_hr_role" /> HR同期の結果</span></div>
      </section>
    </div>

    <section className="surface history-panel"><div className="section-heading"><div><p className="eyebrow">AUDIT TRAIL</p><h2>最近の変更履歴</h2></div><span className="count-chip">直近 {audit.length} 件</span></div>
      {audit.length === 0 ? <p className="muted">監査記録はまだありません。</p> : <div className="table-wrap"><table><thead><tr><th>操作</th><th>対象</th><th>詳細</th><th>日時</th></tr></thead><tbody>{audit.slice(0, 12).map((entry) => <tr key={entry.id}><td><code>{entry.action}</code></td><td>{entry.subject_id?.slice(0, 18) ?? "—"}</td><td className="audit-detail">{JSON.stringify(entry.detail)}</td><td>{new Intl.DateTimeFormat("ja-JP", { timeZone: "Asia/Tokyo", dateStyle: "short", timeStyle: "short" }).format(new Date(entry.occurred_at))}</td></tr>)}</tbody></table></div>}
    </section>
  </div>;
}

function GrantRow({ grant, onRevoke, busy }: { grant: AuthorizationGrant; onRevoke: () => void; busy: boolean }) {
  return <article className="grant-row">
    <div className="grant-person"><span className="user-avatar user-avatar-small">{grant.display_name.slice(0, 1)}</span><div><strong>{grant.display_name}</strong><small>{grant.email ?? grant.user_id.slice(0, 8)}</small></div></div>
    <div className="grant-description"><strong>{grant.permission === "admin" ? "システム管理者" : grant.department_name}</strong><small>{grant.permission === "admin" ? "全社設定・監査" : "出退勤データの閲覧"}</small></div>
    <div className="grant-meta"><GrantSourceBadge source={grant.grant_source} /><small>{grant.grant_source === "manual" ? `付与者 ${grant.granted_by?.slice(0, 8) ?? "不明"}` : "役職・所属データに基づく"}</small></div>
    {grant.grant_source === "manual" ? <button className="button button-quiet button-small revoke-button" type="button" disabled={busy} onClick={onRevoke}>取消</button> : <span className="managed-label">HR管理</span>}
  </article>;
}
