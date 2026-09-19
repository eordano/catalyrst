import { useRef, useState } from "react";
import { hierarchySelection, type HierarchyPlacement } from "../hierarchy-selection";
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

interface TreeActions {
  selectedIds?: string[];
  onChoose?: (id: string, modifiers: { shiftKey?: boolean; ctrlKey?: boolean; metaKey?: boolean }) => void;
  onMove?: (ids: string[], target: string, placement: HierarchyPlacement) => Promise<void>;
  busy?: boolean;
}

interface TreeRowProps extends TreeActions {
  node: DeTreeNode;
  depth: number;
  expandAll?: boolean;
  live?: boolean;
  onSelect?: (id: string | number) => void;
  onFocus?: (id: string | number) => void;
  activeId?: string | number | null;
}

function TreeRow({ node, depth, expandAll = false, live = false, onSelect, onFocus, activeId = null, selectedIds, onChoose, onMove, busy }: TreeRowProps) {
  const kids = node.children ?? [];
  const hasKids = kids.length > 0;
  const [open, setOpen] = useState(node.expanded ?? false);
  const isOpen = expandAll || open;
  const selectable = typeof onSelect === "function" || typeof onChoose === "function";
  const [drop, setDrop] = useState<HierarchyPlacement | null>(null);
  const choose = (e: { shiftKey?: boolean; ctrlKey?: boolean; metaKey?: boolean }) => onChoose ? onChoose(String(node.id), e) : onSelect?.(node.id);
  const placement = (e: React.DragEvent<HTMLDivElement>): HierarchyPlacement => {
    if (String(node.id) === "0") return "inside";
    const rect = e.currentTarget.getBoundingClientRect();
    const ratio = (e.clientY - rect.top) / rect.height;
    return ratio < .25 ? "before" : ratio > .75 ? "after" : "inside";
  };
  const selected =
    selectedIds ? selectedIds.includes(String(node.id)) : node.selected || (activeId != null && String(node.id) === String(activeId));
  return (
    <>
      <div
        className={
          "eui-row" +
          (selected ? " selected" : "") +
          (live && !selectable ? " is-readonly" : "")
        }
        style={{ paddingLeft: 4 + depth * 14, ...(drop === "before" ? { borderTop: "2px solid var(--eui-accent, #9b7cff)" } : drop === "after" ? { borderBottom: "2px solid var(--eui-accent, #9b7cff)" } : drop === "inside" ? { outline: "1px solid var(--eui-accent, #9b7cff)", outlineOffset: -1 } : {}) }}
        data-entity-id={String(node.id)}
        draggable={!!onMove && !busy && String(node.id) !== "0"}
        onDragStart={onMove ? e => {
          e.stopPropagation();
          const ids = selectedIds?.includes(String(node.id)) ? selectedIds.filter(id => id !== "0") : [String(node.id)];
          e.dataTransfer.setData("application/x-dcl-entities", JSON.stringify(ids));
          e.dataTransfer.effectAllowed = "move";
          if (!selected) choose({});
        } : undefined}
        onDragOver={onMove && !busy ? e => {
          if (!Array.from(e.dataTransfer.types).includes("application/x-dcl-entities")) return;
          e.preventDefault();
          e.stopPropagation();
          e.dataTransfer.dropEffect = "move";
          setDrop(placement(e));
        } : undefined}
        onDragLeave={() => setDrop(null)}
        onDrop={onMove && !busy ? e => {
          e.preventDefault();
          e.stopPropagation();
          setDrop(null);
          try {
            const ids: unknown = JSON.parse(e.dataTransfer.getData("application/x-dcl-entities"));
            if (Array.isArray(ids) && ids.every(id => typeof id === "string")) void onMove(ids, String(node.id), placement(e));
          } catch { }
        } : undefined}
        title={node.name}
        role="treeitem"
        aria-level={depth + 1}
        aria-expanded={hasKids ? isOpen : undefined}
        tabIndex={selectable ? 0 : undefined}
        aria-selected={selectable ? selected : undefined}
        onClick={selectable ? e => choose(e) : undefined}
        onDoubleClick={
          typeof onFocus === "function"
            ? (e) => {
                e.preventDefault();
                onFocus(node.id);

              }
            : undefined
        }
        onKeyDown={
          selectable
            ? (e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  choose(e);
                }
                if (e.key === "ArrowRight") { e.preventDefault(); setOpen(true); }
                if (e.key === "ArrowLeft") { e.preventDefault(); setOpen(false); }
                if (e.key === "ArrowDown" || e.key === "ArrowUp") {
                  e.preventDefault();
                  const rows = Array.from(e.currentTarget.closest('[role="tree"]')?.querySelectorAll<HTMLElement>('[data-entity-id]') ?? []);
                  rows[rows.indexOf(e.currentTarget) + (e.key === "ArrowDown" ? 1 : -1)]?.focus();
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
            onFocus={onFocus}
            activeId={activeId}
            selectedIds={selectedIds}
            onChoose={onChoose}
            onMove={onMove}
            busy={busy}
          />
        ))}
    </>
  );
}

interface DeHierarchyPanelProps extends TreeActions {
  onSelectionChange?: (ids: string[], active: string | null) => void;
  onCopy?: () => Promise<void>;
  onPaste?: () => Promise<void>;
  onDuplicate?: () => Promise<void>;
  onUnparent?: () => Promise<void>;
  tree?: DeTreeNode[];
  title?: string;
  width?: number;
  empty?: boolean;
  contextMenu?: DeContextMenuProps | null;
  live?: boolean;
  onSelect?: (id: string | number) => void;
  onFocus?: (id: string | number) => void;
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
  onFocus = undefined,
  activeId = null,
  onAddEntity = undefined,
  onOpenAssets = undefined,
  selectedIds, onSelectionChange, onMove, onCopy, onPaste, onDuplicate, onUnparent,
}: DeHierarchyPanelProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  const anchor = useRef<string | null>(null);
  const pending = useRef(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const act = async (fn: (() => Promise<void>) | undefined) => {
    if (!fn || pending.current) return;
    pending.current = true;
    setBusy(true);
    setError(null);
    try { await fn(); } catch (reason) { setError(reason instanceof Error ? reason.message : "The hierarchy could not be updated. Try again."); }
    finally { pending.current = false; setBusy(false); }
  };
  const onChoose: TreeActions["onChoose"] = onSelectionChange ? (id, modifiers) => {
    const visible = Array.from(panelRef.current?.querySelectorAll<HTMLElement>("[data-entity-id]") ?? []).map(row => row.dataset.entityId!);
    const next = hierarchySelection(visible, selectedIds ?? [], anchor.current, id, modifiers);
    if (!modifiers.shiftKey) anchor.current = id;
    onSelectionChange(next, next.includes(id) ? id : next.at(-1) ?? null);
  } : undefined;
  const [query, setQuery] = useState("");
  const q = query.trim().toLowerCase();
  const filtered = q ? filterTree(tree, q) : tree;
  const total = countNodes(tree);
  const shown = q ? countNodes(filtered) : total;
  const noun = total === 1 ? "entity" : "entities";

  return (
    <div ref={panelRef} className="eui-panel eui-left" style={{ width }} aria-busy={busy} onKeyDown={e => {
      if (!(e.ctrlKey || e.metaKey) || (e.target as HTMLElement).closest("input,textarea,[contenteditable=true]")) return;
      const action = ({ c: onCopy, v: onPaste, d: onDuplicate } as Record<string, (() => Promise<void>) | undefined>)[e.key.toLowerCase()];
      if (action) { e.preventDefault(); e.stopPropagation(); void act(action); }
      if (e.key.toLowerCase() === "a" && onSelectionChange) {
        e.preventDefault(); e.stopPropagation();
        const ids = Array.from(panelRef.current?.querySelectorAll<HTMLElement>("[data-entity-id]") ?? []).map(row => row.dataset.entityId!).filter(id => id !== "0");
        onSelectionChange(ids, ids.at(-1) ?? null);
      }
    }}>
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
      {(onCopy || onPaste || onMove) && <div className="eui-asset-count" style={{ display: "flex", flexWrap: "wrap", gap: 4 }} role="toolbar" aria-label="Hierarchy actions">
        <button className="eui-btn" disabled={!onCopy || busy} onClick={() => void act(onCopy)}>Copy</button>
        <button className="eui-btn" disabled={!onPaste || busy} onClick={() => void act(onPaste)}>Paste</button>
        <button className="eui-btn" disabled={!onDuplicate || busy} onClick={() => void act(onDuplicate)}>Duplicate</button>
        <button className="eui-btn" disabled={!onUnparent || busy} onClick={() => void act(onUnparent)}>Unparent</button>
      </div>}
      {error && <div className="eui-empty" role="alert">{error}</div>}
      {!empty && (
        <div className="eui-asset-count">
          {q ? `${shown} of ${total} ${noun}` : `${total} ${noun}`}
        </div>
      )}
      <div
        className="eui-panel-body"
        style={{ padding: "8px 0" }}
        role="tree"
        aria-multiselectable={!!onSelectionChange}
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
              onFocus={onFocus}
              activeId={activeId}
              selectedIds={selectedIds}
              onChoose={onChoose}
              onMove={onMove ? (ids, target, placement) => act(() => onMove(ids, target, placement)) : undefined}
              busy={busy}
            />
          ))
        )}
      </div>
      {contextMenu && <DeContextMenu {...contextMenu} />}
    </div>
  );
}

interface DeContextMenuProps {
  x?: number;
  y?: number;
  kids?: number;
}

function DeContextMenu({ x = 96, y = 188, kids = 0 }: DeContextMenuProps) {
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
