import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { SyncRun } from "../../api/types";
import { SyncRunProgress } from "./SyncRunProgress";

const failedRun: SyncRun = {
  run_id: "00000000-0000-4000-8000-000000000001", status: "failed", snapshot_hash: "hash", started_at: "2026-11-15T00:00:00Z", completed_at: "2026-11-15T00:00:01Z", error_detail: "employee job_code field was omitted", changed_records: 0, deduplicated: false,
  stages: [
    { name: "fetch", status: "succeeded", started_at: null, completed_at: null, detail: "Loaded 3 departments and 3 employees." },
    { name: "validate", status: "failed", started_at: null, completed_at: null, detail: "employee job_code field was omitted" },
    { name: "departments", status: "skipped", started_at: null, completed_at: null, detail: "Skipped because an earlier stage failed." },
    { name: "people", status: "skipped", started_at: null, completed_at: null, detail: "Skipped because an earlier stage failed." },
    { name: "authorizations", status: "skipped", started_at: null, completed_at: null, detail: "Skipped because an earlier stage failed." },
  ],
};

describe("HR sync progress", () => {
  it("sync_view_renders_stage_and_failure_detail", () => {
    render(<SyncRunProgress run={failedRun} />);
    expect(screen.getByText("必須項目・階層検証")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("employee job_code field was omitted");
    expect(screen.getAllByText("スキップ")).toHaveLength(3);
  });
});
