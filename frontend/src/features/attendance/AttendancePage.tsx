import { useEffect, useState } from "react";
import { getScopedAttendance, getToday } from "../../api/attendanceApi";
import { ApiError } from "../../api/client";
import type { AttendanceRecord, TodayResponse } from "../../api/types";
import { useAuth } from "../../auth/AuthContext";

function timeLabel(value: string | undefined) {
  if (!value) return "—";
  return new Intl.DateTimeFormat("ja-JP", { timeZone: "Asia/Tokyo", hour: "2-digit", minute: "2-digit" }).format(new Date(value));
}

export function AttendancePage() {
  const { session } = useAuth();
  const [today, setToday] = useState<TodayResponse | null>(null);
  const [scopeRecords, setScopeRecords] = useState<AttendanceRecord[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void getToday().then(async (result) => {
      if (!active) return;
      setToday(result);
      const canReadScopes = session?.capabilities.admin || session?.capabilities.manager_department_ids.length;
      if (canReadScopes) {
        try { setScopeRecords(await getScopedAttendance(result.work_date)); }
        catch { if (active) setError("他メンバーの打刻状況を読み込めませんでした。"); }
      }
    }).catch((cause: unknown) => {
      if (active) setError(cause instanceof ApiError ? cause.message : "出退勤情報を読み込めませんでした。");
    });
    return () => { active = false; };
  }, [session?.capabilities.admin, session?.capabilities.manager_department_ids.length]);

  return <div className="page page-attendance">
    <section className="welcome-band">
      <div><p className="eyebrow">PERSONAL ATTENDANCE</p><h1>おはようございます、{session?.user.display_name.split(" ")[0]}さん</h1><p className="welcome-sub">本日の出勤状況を確認できます。QRを読み取ると記録されます。</p></div>
      <div className="date-card"><span className="date-icon">◷</span><span>{new Intl.DateTimeFormat("ja-JP", { timeZone: "Asia/Tokyo", dateStyle: "full" }).format(new Date())}</span></div>
    </section>
    {error && <div className="alert alert-error" role="alert">{error}</div>}
    <div className="attendance-grid">
      <AttendanceCard title="出勤" time={today?.check_in?.occurred_at} status={today?.check_in ? "記録済み" : "未記録"} kind="check-in" />
      <AttendanceCard title="退勤" time={today?.check_out?.occurred_at} status={today?.check_out ? "記録済み" : "未記録"} kind="check-out" />
    </div>
    <section className="surface attendance-note"><div className="note-icon">⌁</div><div><strong>QRコードを読み取って打刻</strong><p>オフィスに掲示された出勤用・退勤用QRをスマートフォンのカメラで読み取ります。アプリのインストールは不要です。</p></div><span className="note-pill">1日各1回</span></section>
    {(session?.capabilities.admin || (session?.capabilities.manager_department_ids.length ?? 0) > 0) && <ManagerAttendancePanel records={scopeRecords} />}
    <section className="privacy-strip"><span className="privacy-check">✓</span><span>打刻時刻はサーバーで記録され、所属部門の閲覧権限に従って共有されます。</span></section>
  </div>;
}

function ManagerAttendancePanel({ records }: { records: AttendanceRecord[] }) {
  return <section className="surface manager-panel"><div className="section-heading"><div><p className="eyebrow">AUTHORIZED DEPARTMENT VIEW</p><h2>他メンバーの打刻状況</h2></div><span className="count-chip">{records.length} 件</span></div>
    {records.length === 0 ? <p className="muted manager-empty">本日の他メンバーの打刻はありません。</p> : <div className="manager-records">{records.map((record) => <article className="manager-record" key={record.id}><span className={`record-kind ${record.kind}`}>{record.kind === "check_in" ? "出勤" : "退勤"}</span><div className="manager-person"><strong>{record.display_name}</strong><small>{record.department_name ?? "所属未設定"}</small></div><time>{timeLabel(record.occurred_at)}</time></article>)}</div>}
  </section>;
}

function AttendanceCard({ title, time, status, kind }: { title: string; time?: string; status: string; kind: "check-in" | "check-out" }) {
  return <section className={`surface attendance-card ${kind}`}>
    <div className="attendance-card-top"><span className="attendance-icon">{kind === "check-in" ? "↗" : "↘"}</span><span className={`status-badge ${time ? "status-done" : "status-pending"}`}><span className="status-dot" />{status}</span></div>
    <p className="attendance-card-label">{title}</p><time className="attendance-time">{timeLabel(time)}</time>
    <p className="attendance-card-hint">{time ? "今日の記録" : "QRコードを読み取ってください"}</p>
  </section>;
}
