import type { SyncStage, StageStatus } from "../../api/types";

export const stageLabels: Record<string, string> = {
  fetch: "HRスナップショット取得",
  validate: "必須項目・階層検証",
  departments: "部門階層・コード同期",
  people: "社員プロフィール・所属同期",
  authorizations: "HR自動権限の差分同期",
};

export function stageStatusLabel(status: StageStatus): string {
  return {
    pending: "未実行",
    running: "実行中",
    succeeded: "成功",
    failed: "失敗",
    skipped: "スキップ",
  }[status];
}

export function sortStages(stages: readonly SyncStage[]): SyncStage[] {
  const order = ["fetch", "validate", "departments", "people", "authorizations"];
  return [...stages].sort((left, right) => order.indexOf(left.name) - order.indexOf(right.name));
}
