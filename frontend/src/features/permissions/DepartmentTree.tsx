import type { Department } from "../../api/types";
import { buildDepartmentTree, toggleDepartmentSelection, type DepartmentNode } from "./departmentTreeModel";

export function DepartmentTree({ departments, selectedIds, onChange }: { departments: Department[]; selectedIds: string[]; onChange: (next: string[]) => void }) {
  const roots = buildDepartmentTree(departments);
  if (roots.length === 0) return <p className="muted">部門データがありません。</p>;
  return <ul className="department-tree">{roots.map((node) => <TreeNode key={node.department.id} node={node} selectedIds={selectedIds} onChange={onChange} />)}</ul>;
}

function TreeNode({ node, selectedIds, onChange }: { node: DepartmentNode; selectedIds: string[]; onChange: (next: string[]) => void }) {
  const department = node.department;
  const checked = selectedIds.includes(department.id);
  const descendantsSelected = node.children.some((child) => child.department.active && selectedIds.includes(child.department.id));
  return <li>
    <label className={`tree-item ${!department.active ? "tree-inactive" : ""}`}>
      <input type="checkbox" checked={checked} disabled={!department.active} aria-label={department.name} onChange={(event) => onChange(toggleDepartmentSelection(selectedIds, node, event.target.checked))} />
      <span className="tree-name">{department.name}</span><span className="tree-members">{department.member_count} 名</span>
      {descendantsSelected && !checked && <span className="tree-partial">一部選択</span>}
      {!department.active && <span className="tree-inactive-label">停止中</span>}
    </label>
    {node.children.length > 0 && <ul>{node.children.map((child) => <TreeNode key={child.department.id} node={child} selectedIds={selectedIds} onChange={onChange} />)}</ul>}
  </li>;
}
