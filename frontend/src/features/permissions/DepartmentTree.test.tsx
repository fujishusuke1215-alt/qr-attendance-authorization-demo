import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Department } from "../../api/types";
import { DepartmentTree } from "./DepartmentTree";

const departments: Department[] = [
  { id: "root", identity_code: "ROOT", name: "本社", parent_id: null, active: true, member_count: 3 },
  { id: "active", identity_code: "ENG", name: "開発部", parent_id: "root", active: true, member_count: 2 },
  { id: "inactive", identity_code: "OLD", name: "旧組織", parent_id: "root", active: false, member_count: 0 },
  { id: "other", identity_code: "OPS", name: "運用部", parent_id: null, active: true, member_count: 1 },
];

describe("department permission tree", () => {
  it("department_tree_selects_active_descendants_only", () => {
    const onChange = vi.fn();
    render(<DepartmentTree departments={departments} selectedIds={["other"]} onChange={onChange} />);
    fireEvent.click(screen.getByRole("checkbox", { name: "本社" }));
    expect(onChange).toHaveBeenCalledWith(["other", "root", "active"]);
    expect(screen.getByRole("checkbox", { name: "旧組織" })).toBeDisabled();
  });
});
