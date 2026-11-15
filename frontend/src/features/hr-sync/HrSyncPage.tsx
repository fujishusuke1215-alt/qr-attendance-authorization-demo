import { useCallback, useEffect, useRef, useState } from "react";
import { getLatestSync, getSyncRun, startSync } from "../../api/adminApi";
import { ApiError } from "../../api/client";
import type { SyncRun } from "../../api/types";
import { SyncRunProgress } from "./SyncRunProgress";

export function HrSyncPage() {
  const [run, setRun] = useState<SyncRun | null>(null);
  const [loading, setLoading] = useState(true);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const pollTimer = useRef<number | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try { setRun(await getLatestSync()); }
    catch (cause) { setError(cause instanceof ApiError && cause.status === 403 ? "管理者権限がありません。" : "同期状況を取得できませんでした。"); }
    finally { setLoading(false); }
  }, []);

  useEffect(() => {
    void refresh();
    return () => { if (pollTimer.current !== null) window.clearTimeout(pollTimer.current); };
  }, [refresh]);

  async function poll(runId: string) {
    try {
      const latest = await getSyncRun(runId);
      setRun(latest);
      if (latest.status === "running") pollTimer.current = window.setTimeout(() => void poll(runId), 800);
    } catch { setError("同期の進行状況を読み込めませんでした。管理画面を再読み込みしてください。"); }
  }

  async function beginSync() {
    setStarting(true);
    setError(null);
    try {
      const response = await startSync();
      await poll(response.run_id);
    } catch (cause) {
      setError(cause instanceof ApiError && cause.status === 403 ? "管理者権限がありません。" : "手動同期を開始できませんでした。");
    } finally { setStarting(false); }
  }

  return <div className="page">
    <header className="page-title sync-page-title"><div><p className="eyebrow">EXTERNAL HR / SYNC STATUS</p><h1>HRデータ同期</h1><p>架空のHRスナップショットを段階的に検証・反映します。必須項目をすべて検証してから、部門・社員・権限の更新を開始します。</p></div><button className="button button-primary" type="button" onClick={() => void beginSync()} disabled={starting || run?.status === "running"}>{starting || run?.status === "running" ? <><span className="spinner spinner-small" />同期中</> : "手動同期を開始"}</button></header>
    {error && <div className="alert alert-error" role="alert">{error}</div>}
    <div className="sync-overview-grid">
      <section className="surface sync-source-card"><span className="source-icon">⇄</span><div><p className="eyebrow">SOURCE ADAPTER</p><h2>Local JSON fixture</h2><p>実HRサービスに接続せず、再現可能なサンプルスナップショットを読み込みます。</p></div><span className="fixture-badge">DEMO DATA</span></section>
      <section className="surface sync-rule-card"><span className="rule-icon">✓</span><div><p className="eyebrow">SAFE RECONCILIATION</p><h2>検証後に反映</h2><p>必須の職務コード項目が欠落すると同期を失敗させ、自動権限を取り消しません。</p></div></section>
    </div>
    {loading && !run ? <div className="surface loading-card"><span className="spinner" />同期履歴を読み込んでいます</div> : run ? <SyncRunProgress run={run} /> : <section className="surface empty-sync"><div className="empty-icon">⇄</div><h2>同期履歴はありません</h2><p>手動同期を開始すると、実行段階と結果がここに表示されます。</p></section>}
    <section className="surface sync-policy"><p className="eyebrow">SYNC POLICY</p><div className="policy-points"><p><strong>部門識別</strong><span>identity_codeを使用し、名称変更・親変更後も同じ部門IDを保ちます。</span></p><p><strong>移行コード</strong><span>別名コードはHR入力に明示された場合だけ同じ部門へ紐付けます。</span></p><p><strong>権限の独立性</strong><span>同期はHR自動付与だけを追加・取消しし、手動付与には触れません。</span></p><p><strong>再実行</strong><span>同じ検証済みスナップショットはno-opとして扱い、重複監査記録を作りません。</span></p></div></section>
  </div>;
}
