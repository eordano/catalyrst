import type { EditorTool } from "../bus-protocol";
import type { DeInspector, DeTreeNode } from "../types";
import DclEditorChrome from "../frames/DclEditorChrome";
import Modal from "../../components/Modal";
import { DeToolbar } from "./DeToolbar";
import { DeHierarchyPanel } from "./DeHierarchyPanel";
import { DeInspectorPanel } from "./DeInspectorPanel";

export interface DeNewEntityModalProps {
  parent?: "root" | "active";
  parentName?: string;
  defaultName?: string;
  onCancel?: () => void;
  onCreate?: (name: string) => void;
  onPickParent?: (parent: "root" | "active") => void;
}

export function DeNewEntityModal({
  parent = "active",
  parentName = "Display Cube",
  defaultName = "",
  onCancel = undefined,
  onCreate = undefined,
  onPickParent = undefined,
}: DeNewEntityModalProps) {
  let nameRef: HTMLInputElement | null = null;
  return (
    <Modal
      className="eui-modal modal__card--plain"
      onClose={onCancel}
      showClose={false}
      ariaLabel="New entity"
    >
      <div className="eui-modal-head">New entity</div>
      <div className="eui-modal-body">
        <input
          className="eui-input"
          placeholder="Entity name"
          aria-label="Entity name"
          defaultValue={defaultName}
          autoFocus
          ref={(el) => { nameRef = el; }}
        />
        {onPickParent ? (
          <div style={{ display: "flex", gap: 8 }}>
            <button
              className={"eui-btn" + (parent === "root" ? " active" : "")}
              aria-pressed={parent === "root"}
              onClick={() => onPickParent("root")}
            >
              At scene root
            </button>
            <button
              className={"eui-btn" + (parent === "active" ? " active" : "")}
              aria-pressed={parent === "active"}
              onClick={() => onPickParent("active")}
            >
              Child of {parentName}
            </button>
          </div>
        ) : null}
      </div>
      <div className="eui-modal-foot">
        <button className="eui-btn" onClick={onCancel} disabled={!onCancel}>Cancel</button>
        <button
          className="eui-btn primary"
          onClick={onCreate ? () => onCreate(nameRef?.value ?? "") : undefined}
          disabled={!onCreate}
        >
          Create
        </button>
      </div>
    </Modal>
  );
}

export interface DeNewEntityDialogProps {
  parent?: "root" | "active";
  title?: string;
  tree?: DeTreeNode[];
  inspector?: DeInspector;
  viewportSrc?: string | null;
  parentName?: string;
  defaultName?: string;
  live?: boolean;
  tool?: EditorTool;
  onTool?: (tool: EditorTool) => void;
  hideLeft?: boolean;
  hideRight?: boolean;
  onCancel?: () => void;
  onCreate?: (name: string) => void;
  onPickParent?: (parent: "root" | "active") => void;
}

export default function DeNewEntityDialog({
  parent = "active",
  title,
  tree,
  inspector = {},
  viewportSrc = null,
  parentName = "Display Cube",
  defaultName = "",
  live = Boolean(viewportSrc),
  tool = "translate",
  onTool = undefined,
  hideLeft = false,
  hideRight = false,
  onCancel = undefined,
  onCreate = undefined,
  onPickParent = undefined,
}: DeNewEntityDialogProps) {
  return (
    <DclEditorChrome viewportSrc={viewportSrc}>
      <DeToolbar live={live} tool={tool} onTool={onTool} hideLeft={hideLeft} hideRight={hideRight} />
      {!hideLeft && <DeHierarchyPanel title={title} tree={tree} live={live} />}
      {!hideRight && (
        <DeInspectorPanel
          name={inspector.name}
          id={inspector.id}
          components={inspector.components}
          transform={inspector.transform}
          live={live}
        />
      )}

      <DeNewEntityModal
        parent={parent}
        parentName={parentName}
        defaultName={defaultName}
        onCancel={onCancel}
        onCreate={onCreate}
        onPickParent={onPickParent}
      />
    </DclEditorChrome>
  );
}
