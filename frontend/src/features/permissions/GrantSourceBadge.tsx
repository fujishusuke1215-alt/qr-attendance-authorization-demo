import type { GrantSource } from "../../api/types";

export function GrantSourceBadge({ source }: { source: GrantSource }) {
  return <span className={`grant-source grant-source-${source}`}>
    <span className="grant-source-dot" />{source === "manual" ? "手動付与" : "HR自動付与"}
  </span>;
}
