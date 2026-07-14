import { useState } from "react";
import type { DeTreeNode } from "../types";
import ContextMenu from "../../components/ContextMenu";
import { IconCamera, IconEdit, IconImport, IconPlus, IconTrash } from "./DeIcons";

function countNodes(nodes: DeTreeNode[]): number {
  let total = 0;
  for (const node of nodes) {
    total += 1;
    const kids = node.children ?? [];
    if (kids.length) total += countNodes(kids);
  }
  return total;
}

function filterTree(nodes: DeTreeNode[], q: string): DeTreeNode[] {
  const out: DeTreeNode[] = [];
  for (const node of nodes) {
    const kids = filterTree(node.children ?? [], q);
    if (node.name.toLowerCase().includes(q) || kids.length) {
      out.push({ ...node, children: kids });
    }
  }
  return out;
}

interface TreeRowProps {
  node: DeTreeNode;
  depth: number;
  expandAll?: boolean;
  live?: boolean;
  onSelect?: (id: string | number) => void;
  activeId?: string | number | null;
}

function TreeRow({ node, depth, expandAll = false, live = false, onSelect, activeId = null }: TreeRowProps) {
  const kids = node.children ?? [];
  const hasKids = kids.length > 0;
  const [open, setOpen] = useState(node.expanded ?? false);
  const isOpen = expandAll || open;
  const selectable = typeof onSelect === "function";
  const selected =
    node.selected || (activeId != null && String(node.id) === String(activeId));
  return (
    <>
      <div
        className={
          "eui-row" +
          (selected ? " selected" : "") +
          (live && !selectable ? " is-readonly" : "")
        }
        style={{ paddingLeft: 4 + depth * 14 }}
        title={node.name}
        role={selectable ? "button" : undefined}
        tabIndex={selectable ? 0 : undefined}
        aria-pressed={selectable ? selected : undefined}
        onClick={selectable ? () => onSelect?.(node.id) : undefined}
        onKeyDown={
          selectable
            ? (e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onSelect?.(node.id);
                }
              }
            : undefined
        }
      >
        <span
          className="twisty"
          onClick={hasKids ? (e) => { e.stopPropagation(); setOpen((v) => !v); } : undefined}
        >
          {hasKids ? (isOpen ? "\u{25BE}" : "\u{25B8}") : ""}
        </span>
        <span className="label">
          {node.name}
          {hasKids && <span className="dim">{kids.length}</span>}
        </span>
      </div>
      {isOpen &&
        kids.map((c) => (
          <TreeRow
            key={c.id}
            node={c}
            depth={depth + 1}
            expandAll={expandAll}
            live={live}
            onSelect={onSelect}
            activeId={activeId}
          />
        ))}
    </>
  );
}

export interface DeHierarchyPanelProps {
  tree?: DeTreeNode[];
  title?: string;
  width?: number;
  empty?: boolean;
  contextMenu?: DeContextMenuProps | null;
  live?: boolean;
  onSelect?: (id: string | number) => void;
  activeId?: string | number | null;
  onAddEntity?: () => void;
  onOpenAssets?: () => void;
}

export function DeHierarchyPanel({
  tree = [],
  title = "",
  width = 300,
  empty = false,
  contextMenu = null,
  live = false,
  onSelect,
  activeId = null,
  onAddEntity = undefined,
  onOpenAssets = undefined,
}: DeHierarchyPanelProps) {
  const [query, setQuery] = useState("");
  const q = query.trim().toLowerCase();
  const filtered = q ? filterTree(tree, q) : tree;
  const total = countNodes(tree);
  const shown = q ? countNodes(filtered) : total;
  const noun = total === 1 ? "entity" : "entities";

  return (
    <div className="eui-panel eui-left" style={{ width }}>
      <div className="eui-panel-head">
        <div className="eui-head-text">
          <span className="eui-overline">Scene</span>
          <span className="eui-title" title={title}>{title}</span>
        </div>
        <button
          className="eui-btn icon"
          title="Browse asset catalog"
          aria-label="Browse asset catalog"
          onClick={onOpenAssets}
          disabled={!onOpenAssets}
        >
          <IconImport />
        </button>
        <button
          className="eui-btn icon"
          title="New entity"
          aria-label="New entity"
          onClick={onAddEntity ? () => onAddEntity() : undefined}
          disabled={!onAddEntity}
        >
          <IconPlus />
        </button>
      </div>
      <div className="eui-search">
        <input
          className="eui-input"
          placeholder={"Search entities\u{2026}"}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          spellCheck={false}
        />
      </div>
      {!empty && (
        <div className="eui-asset-count">
          {q ? `${shown} of ${total} ${noun}` : `${total} ${noun}`}
        </div>
      )}
      <div
        className="eui-panel-body"
        style={{ padding: "8px 0" }}
        role="region"
        aria-label="Scene hierarchy"
        tabIndex={0}
      >
        {empty ? (
          <div className="eui-empty">No named entities yet &#x2014; create one with +</div>
        ) : filtered.length === 0 ? (
          <div className="eui-empty">
            {q ? `No entities match \u{201C}${query.trim()}\u{201D}` : "No named entities yet \u{2014} place one from the catalog"}
          </div>
        ) : (
          filtered.map((node) => (
            <TreeRow
              key={node.id}
              node={node}
              depth={0}
              expandAll={!!q}
              live={live}
              onSelect={onSelect}
              activeId={activeId}
            />
          ))
        )}
      </div>
      {contextMenu && <DeContextMenu {...contextMenu} />}
    </div>
  );
}

export interface DeContextMenuProps {
  x?: number;
  y?: number;
  kids?: number;
}

export function DeContextMenu({ x = 96, y = 188, kids = 0 }: DeContextMenuProps) {
  return (
    <div className="eui-ctx" style={{ left: x, top: y }}>
      <ContextMenu
        items={[
          { kind: "button", label: "Focus camera", icon: <IconCamera /> },
          { kind: "button", label: "Rename", icon: <IconEdit /> },
          { kind: "button", label: "New child entity", icon: <IconPlus /> },
          { kind: "button", label: "Duplicate", icon: <IconPlus /> },
          { kind: "separator" },
          { kind: "button", label: "Unparent" },
          { kind: "separator" },
          ...(kids === 0
            ? [{ kind: "button" as const, label: "Delete", icon: <IconTrash />, danger: true }]
            : [
                { kind: "button" as const, label: "Delete, keep children", icon: <IconTrash />, danger: true },
                { kind: "button" as const, label: `Delete with ${kids} child${kids === 1 ? "" : "ren"}`, icon: <IconTrash />, danger: true },
              ]),
        ]}
      />
    </div>
  );
}
