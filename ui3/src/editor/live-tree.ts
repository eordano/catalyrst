import type { EditorEntityNode } from "./bus-protocol";
import type { DeTreeNode } from "./types";

interface LiveTreeNode {
  id: string;
  name: string;
  expanded?: boolean;
  children: LiveTreeNode[];
}

export function buildLiveTree(
  entities: EditorEntityNode[] | null | undefined,
  rootLabel = "Scene",
): DeTreeNode[] {
  const byId = new Map<string, LiveTreeNode>();
  for (const e of entities ?? []) {
    byId.set(String(e.id), {
      id: String(e.id),
      name: e.name || `Entity ${e.id}`,
      children: [],
    });
  }
  const root: LiveTreeNode = { id: "0", name: rootLabel, expanded: true, children: [] };
  for (const e of entities ?? []) {
    const node = byId.get(String(e.id))!;
    const pid = e.parent == null ? "0" : String(e.parent);
    const parent = pid !== String(e.id) ? byId.get(pid) : undefined;
    if (parent) {
      parent.children.push(node);
      parent.expanded = true;
    } else {
      root.children.push(node);
    }
  }
  return [root];
}

export function findNodeName(
  nodes: DeTreeNode[] | null | undefined,
  id: string | number | null | undefined,
): string | null {
  if (id == null) return null;
  const target = String(id);
  for (const node of nodes ?? []) {
    if (String(node.id) === target) return node.name;
    const kid = findNodeName(node.children ?? [], target);
    if (kid != null) return kid;
  }
  return null;
}
