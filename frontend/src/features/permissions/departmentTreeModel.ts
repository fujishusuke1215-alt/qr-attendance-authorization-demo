import type { Department } from "../../api/types";

export interface DepartmentNode {
  department: Department;
  children: DepartmentNode[];
}

export function buildDepartmentTree(departments: readonly Department[]): DepartmentNode[] {
  const nodes = new Map(departments.map((department) => [department.id, { department, children: [] as DepartmentNode[] }]));
  const roots: DepartmentNode[] = [];
  for (const department of departments) {
    const node = nodes.get(department.id)!;
    const parent = department.parent_id ? nodes.get(department.parent_id) : undefined;
    if (parent && parent !== node) parent.children.push(node);
    else roots.push(node);
  }
  const sort = (items: DepartmentNode[]) => {
    items.sort((left, right) => left.department.name.localeCompare(right.department.name, "ja"));
    for (const item of items) sort(item.children);
  };
  sort(roots);
  return roots;
}

export function descendants(node: DepartmentNode): Department[] {
  return [node.department, ...node.children.flatMap(descendants)];
}

export function toggleDepartmentSelection(selectedIds: readonly string[], node: DepartmentNode, checked: boolean): string[] {
  const result = new Set(selectedIds);
  for (const department of descendants(node)) {
    if (checked && department.active) result.add(department.id);
    else result.delete(department.id);
  }
  return [...result];
}
