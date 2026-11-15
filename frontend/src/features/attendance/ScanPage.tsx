import { useEffect, useRef, useState } from "react";
import { useParams } from "react-router-dom";
import { apiRequest } from "../../api/client";
import { createAttendance } from "../../api/attendanceApi";
import type { AttendanceRecord, CurrentSession } from "../../api/types";
import { useAuth } from "../../auth/AuthContext";
import { getOrCreateIdempotencyKey, submitAttendanceAfterProfile } from "./attendanceFlow";

type ScanState = { status: "working" } | { status: "done"; created: boolean; event: AttendanceRecord } | { status: "error"; message: string };

export function ScanPage() {
  const { token = "" } = useParams();
  const { session } = useAuth();
  const attempt = useRef<Promise<{ created: boolean; event: AttendanceRecord }> | null>(null);
  const [state, setState] = useState<ScanState>({ status: "working" });

  useEffect(() => {
    if (!session || !token) return;
    attempt.current ??= submitAttendanceAfterProfile(
      token,
      getOrCreateIdempotencyKey(token),
      () => apiRequest<CurrentSession>("/me").then((result) => result.user),
      createAttendance,
    );
    void attempt.current.then((result) => setState({ status: "done", ...result })).catch((cause: unknown) => {
      const message = cause instanceof Error ? cause.message : "出退勤を記録できませんでした。QRを確認して再度お試しください。";
      setState({ status: "error", message });
    });
  }, [session, token]);

  return <div className="scan-page">
    <section className="scan-card surface">
      {state.status === "working" ? <><span className="scan-symbol scan-progress"><span className="spinner" /></span><p className="eyebrow">ATTENDANCE</p><h1>打刻を確認しています</h1><p className="muted">社員プロフィールとQRの有効性を確認しています。</p></> : null}
      {state.status === "done" ? <><span className="scan-symbol scan-success">✓</span><p className="eyebrow">{state.event.kind === "check_in" ? "CHECK IN" : "CHECK OUT"}</p><h1>{state.created ? "打刻を記録しました" : "本日は記録済みです"}</h1><p className="muted">{state.event.display_name} さんの{state.event.kind === "check_in" ? "出勤" : "退勤"}時刻</p><time className="scan-time">{new Intl.DateTimeFormat("ja-JP", { timeZone: "Asia/Tokyo", hour: "2-digit", minute: "2-digit" }).format(new Date(state.event.occurred_at))}</time><p className="scan-date">{state.event.work_date} ・ Asia/Tokyo</p></> : null}
      {state.status === "error" ? <><span className="scan-symbol scan-error">!</span><p className="eyebrow">CHECK FAILED</p><h1>打刻できませんでした</h1><div className="alert alert-error" role="alert">{state.message}</div><p className="muted">QRが更新済みの場合は、掲示中の新しいQRを読み取ってください。</p></> : null}
      <a className="button button-quiet scan-back" href="/attendance">出退勤画面へ戻る</a>
    </section>
  </div>;
}
