import type { SyncRun } from "../../api/types";
import { sortStages, stageLabels, stageStatusLabel } from "./syncView";

export function SyncRunProgress({ run }: { run: SyncRun }) {
  return <section className="surface sync-progress" aria-label="HR同期の段階">
    <div className="section-heading"><div><p className="eyebrow">SYNC RUN</p><h2>{run.status === "running" ? "同期を実行中" : run.status === "failed" ? "同期に失敗しました" : "同期が完了しました"}</h2></div><span className={`run-status run-${run.status}`}><span className="status-dot" />{run.status === "running" ? "実行中" : run.status === "failed" ? "失敗" : run.deduplicated ? "変更なし" : "成功"}</span></div>
    {run.error_detail && <div className="alert alert-error sync-error-detail" role="alert">{run.error_detail}</div>}
    {run.deduplicated && <div className="alert alert-info">同じスナップショットを確認済みのため、データと監査履歴は変更していません。</div>}
    <ol className="sync-stage-list">{sortStages(run.stages).map((stage, index) => <li className={`sync-stage stage-${stage.status}`} key={stage.name}>
      <span className="stage-marker">{stage.status === "succeeded" ? "✓" : stage.status === "failed" ? "!" : stage.status === "running" ? <span className="spinner spinner-small" /> : String(index + 1).padStart(2, "0")}</span>
      <div className="stage-body"><div className="stage-topline"><strong>{stageLabels[stage.name] ?? stage.name}</strong><span className={`stage-state status-${stage.status}`}>{stageStatusLabel(stage.status)}</span></div>
        {stage.detail && <p className={stage.status === "failed" ? "stage-detail stage-failure-detail" : "stage-detail"}>{stage.detail}</p>}
      </div>
      {index < run.stages.length - 1 && <span className="stage-connector" />}
    </li>)}</ol>
    <div className="sync-run-footer"><span>実行ID <code>{run.run_id}</code></span><span>{run.changed_records} 件の変更</span><span>{new Intl.DateTimeFormat("ja-JP", { timeZone: "Asia/Tokyo", dateStyle: "medium", timeStyle: "short" }).format(new Date(run.started_at))}</span></div>
  </section>;
}
