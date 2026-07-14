import type { ReactNode } from "react";
import { useId, useMemo, useState } from "react";
import { SitesChromeMaybe } from "../frames/SitesChrome";
import Modal from "../../components/Modal";
import "./ststoragescene.css";

export type SceneKey = { key: string };

const AddIcon = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor" aria-hidden="true">
    <path d="M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2Z" />
  </svg>
);
const DeleteSweepIcon = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor" aria-hidden="true">
    <path d="M15 16h4v2h-4v-2Zm0-8h7v2h-7V8Zm0 4h6v2h-6v-2ZM3 18c0 1.1.9 2 2 2h6c1.1 0 2-.9 2-2V8H3v10ZM14 5h-3l-1-1H6L5 5H2v2h12V5Z" />
  </svg>
);
const EditIcon = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor" aria-hidden="true">
    <path d="M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25ZM20.71 7.04a1 1 0 0 0 0-1.41l-2.34-2.34a1 1 0 0 0-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83Z" />
  </svg>
);
const DeleteIcon = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor" aria-hidden="true">
    <path d="M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12ZM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4Z" />
  </svg>
);
const ArrowBackIcon = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor" aria-hidden="true">
    <path d="M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2Z" />
  </svg>
);
const FmdGoodIcon = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor" aria-hidden="true">
    <path d="M12 2c-3.87 0-7 3.13-7 7 0 5.25 7 13 7 13s7-7.75 7-13c0-3.87-3.13-7-7-7Zm0 9.5a2.5 2.5 0 1 1 0-5 2.5 2.5 0 0 1 0 5Z" />
  </svg>
);
const SettingsIcon = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
    <path d="M19.14 12.94a7.5 7.5 0 0 0 .06-.94 7.5 7.5 0 0 0-.06-.94l2.03-1.58a.5.5 0 0 0 .12-.61l-1.92-3.32a.5.5 0 0 0-.59-.22l-2.39.96a7 7 0 0 0-1.62-.94l-.36-2.54a.49.49 0 0 0-.5-.42h-3.84a.49.49 0 0 0-.49.42l-.36 2.54a7 7 0 0 0-1.62.94l-2.39-.96a.5.5 0 0 0-.6.22L2.74 8.87a.5.5 0 0 0 .12.61l2.03 1.58a7.6 7.6 0 0 0 0 1.88l-2.03 1.58a.5.5 0 0 0-.12.61l1.92 3.32a.5.5 0 0 0 .6.22l2.39-.96c.5.38 1.04.7 1.62.94l.36 2.54c.04.24.25.42.49.42h3.84c.25 0 .46-.18.5-.42l.36-2.54c.58-.24 1.12-.56 1.62-.94l2.39.96a.5.5 0 0 0 .59-.22l1.92-3.32a.5.5 0 0 0-.12-.61l-2.03-1.58ZM12 15.5a3.5 3.5 0 1 1 0-7 3.5 3.5 0 0 1 0 7Z" />
  </svg>
);
const ViewInArIcon = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
    <path d="M3 4a1 1 0 0 1 1-1h2v2H5v1H3V4Zm0 4h2v3H3V8Zm0 5h2v3H3v-3Zm0 5h2v1h1v2H4a1 1 0 0 1-1-1v-2Zm5 3h3v-2H8v2Zm5 0h3v-2h-3v2Zm5 0h2a1 1 0 0 0 1-1v-2h-2v1h-1v2Zm3-5h-2v-3h2v3Zm0-5h-2V8h2v3Zm0-5V4a1 1 0 0 0-1-1h-2v2h1v1h2ZM13 3h-3v2h3V3ZM8 3h3v2H8V3Zm4 5.27 4 2.31v4.62l-4 2.31-4-2.31v-4.62l4-2.31Z" />
  </svg>
);
const PeopleIcon = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
    <path d="M16 11c1.66 0 2.99-1.34 2.99-3S17.66 5 16 5s-3 1.34-3 3 1.34 3 3 3Zm-8 0c1.66 0 2.99-1.34 2.99-3S9.66 5 8 5 5 6.34 5 8s1.34 3 3 3Zm0 2c-2.33 0-7 1.17-7 3.5V19h14v-2.5c0-2.33-4.67-3.5-7-3.5Zm8 0c-.29 0-.62.02-.97.05 1.16.84 1.97 1.97 1.97 3.45V19h6v-2.5c0-2.33-4.67-3.5-7-3.5Z" />
  </svg>
);
const Spinner = () => (
  <span className="ss__spinner" role="progressbar" aria-label="Loading" />
);

const STORAGE_TABS = [
  { value: "env", label: "Environment", icon: <SettingsIcon /> },
  { value: "scene", label: "Scene", icon: <ViewInArIcon /> },
  { value: "players", label: "Player", icon: <PeopleIcon /> },
];

function StorageLayout({
  realm,
  position,
  active = "scene",
  children,
}: {
  realm?: string | null;
  position?: string | null;
  active?: string;
  children?: ReactNode;
}) {
  const scopeLabel = realm ?? position ?? "";
  return (
    <div className="ss__container">
      <div className="ss__header">
        <button type="button" className="ss__back" aria-label="Back">
          <ArrowBackIcon />
          <span className="ss__back-text">Back</span>
        </button>
        {scopeLabel ? (
          <div className="ss__scoperow">
            <span className="ss__scopechip">
              <FmdGoodIcon />
              <span className="ss__scopelabel">{scopeLabel}</span>
            </span>
            {realm && position ? (
              <span className="ss__scopepos">Position: {position}</span>
            ) : null}
          </div>
        ) : null}
      </div>
      <div className="ss__tabsroot">
        <div className="ss__tabs" role="tablist" aria-label="storage sections">
          {STORAGE_TABS.map((tab) => (
            <button
              key={tab.value}
              type="button"
              role="tab"
              aria-selected={tab.value === active}
              className={"ss__tab" + (tab.value === active ? " is-active" : "")}
            >
              <span className="ss__tab-icon" aria-hidden="true">{tab.icon}</span>
              {tab.label}
            </button>
          ))}
        </div>
      </div>
      {children}
    </div>
  );
}

function KeyTable({
  keys,
  emptyLabel,
  onEdit,
  onDelete,
}: {
  keys: SceneKey[];
  emptyLabel: string;
  onEdit: (key: string) => void;
  onDelete: (key: string) => void;
}) {
  if (keys.length === 0) {
    return (
      <div className="ss__paper ss__empty">
        <span className="ss__empty-text">{emptyLabel}</span>
      </div>
    );
  }
  return (
    <div className="ss__paper ss__tablewrap">
      <table className="ss__table">
        <thead>
          <tr>
            <th className="ss__th">Key</th>
            <th className="ss__th ss__th--right">Actions</th>
          </tr>
        </thead>
        <tbody>
          {keys.map((item) => (
            <tr key={item.key} className="ss__row">
              <td className="ss__td ss__td--key">{item.key}</td>
              <td className="ss__td ss__td--right">
                <button
                  type="button"
                  className="ss__icbtn ss__icbtn--primary"
                  aria-label={`edit ${item.key}`}
                  onClick={() => onEdit(item.key)}
                >
                  <EditIcon />
                </button>
                <button
                  type="button"
                  className="ss__icbtn ss__icbtn--error"
                  aria-label={`delete ${item.key}`}
                  onClick={() => onDelete(item.key)}
                >
                  <DeleteIcon />
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function DialogScrim({
  onClose,
  maxWidth = "sm",
  labelledBy,
  portal,
  children,
}: {
  onClose?: () => void;
  maxWidth?: string;
  labelledBy?: string;
  portal?: boolean;
  children?: ReactNode;
}) {
  return (
    <Modal
      onClose={onClose}
      width="100%"
      className={"ss-dialog ss-dialog--" + maxWidth}
      ariaLabelledBy={labelledBy}
      portal={portal}
    >
      {children}
    </Modal>
  );
}

function SceneValueDialog({
  mode,
  keyName = "",
  initialValue = "",
  onClose,
  onSave,
  portal,
}: {
  mode: "add" | "edit";
  keyName?: string;
  initialValue?: string;
  onClose?: () => void;
  onSave: (payload: { key: string; value: string }) => void;
  portal?: boolean;
}) {
  const titleId = useId();
  const isEdit = mode === "edit";
  const [newKey, setNewKey] = useState(isEdit ? keyName : "");
  const [value, setValue] = useState(initialValue);

  const trimmed = value.trim();
  const looksJson = trimmed.startsWith("{") || trimmed.startsWith("[");
  let jsonError = false;
  if (looksJson) {
    try {
      JSON.parse(trimmed);
    } catch {
      jsonError = true;
    }
  }
  const valueValid = Boolean(trimmed) && !jsonError;
  const canSave = isEdit ? valueValid : Boolean(newKey.trim()) && valueValid;

  return (
    <DialogScrim onClose={onClose} maxWidth={isEdit ? "md" : "sm"} labelledBy={titleId} portal={portal}>
      <h2 className="ss-dialog__title" id={titleId}>
        {isEdit ? `Edit Value: ${keyName}` : "Add Scene Value"}
      </h2>
      <div className="ss-dialog__content">
        {!isEdit ? (
          <label className="ss-field">
            <span className="ss-field__label">Key</span>
            <input
              className="ss-field__input"
              autoFocus
              value={newKey}
              onChange={(e) => setNewKey(e.target.value)}
            />
          </label>
        ) : null}
        <label className="ss-field">
          <span className="ss-field__label">Value</span>
          <textarea
            className={"ss-field__textarea" + (jsonError ? " is-error" : "")}
            rows={isEdit ? 12 : 4}
            placeholder={isEdit ? undefined : "Plain text or JSON: {value}"}
            value={value}
            onChange={(e) => setValue(e.target.value)}
          />
        </label>
        {jsonError ? <div className="ss-alert ss-alert--error">Invalid JSON</div> : null}
      </div>
      <div className="ss-dialog__actions">
        <button type="button" className="ss-btn ss-btn--text" onClick={onClose}>
          Cancel
        </button>
        <button
          type="button"
          className="ss-btn ss-btn--contained"
          disabled={!canSave}
          onClick={() => onSave({ key: isEdit ? keyName : newKey.trim(), value })}
        >
          Save
        </button>
      </div>
    </DialogScrim>
  );
}

function ConfirmDialog({
  title,
  message,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  onConfirm,
  onCancel,
  portal,
}: {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  onConfirm?: () => void;
  onCancel?: () => void;
  portal?: boolean;
}) {
  const titleId = useId();
  return (
    <DialogScrim onClose={onCancel} maxWidth="xs" labelledBy={titleId} portal={portal}>
      <h2 className="ss-dialog__title" id={titleId}>{title}</h2>
      <div className="ss-dialog__content">
        <p className="ss-dialog__text">{message}</p>
      </div>
      <div className="ss-dialog__actions">
        <button type="button" className="ss-btn ss-btn--text" onClick={onCancel} aria-label={cancelLabel}>
          {cancelLabel}
        </button>
        <button
          type="button"
          className="ss-btn ss-btn--contained ss-btn--error"
          onClick={onConfirm}
          aria-label={confirmLabel}
        >
          {confirmLabel}
        </button>
      </div>
    </DialogScrim>
  );
}

type StStorageSceneProps = {
  chrome?: boolean;
  sceneKeys?: SceneKey[];
  loading?: boolean;
  realm?: string | null;
  position?: string | null;
  initialDialog?: "add" | "edit" | null;
  embedded?: boolean;
  /** Forwarded to `Modal`. `false` renders the dialogs in place instead of portalling them. */
  portal?: boolean;
};

export default function StStorageScene({
  chrome = true,
  sceneKeys = [],
  loading = false,
  realm = null,
  position = null,
  initialDialog = null,
  embedded = false,
  portal = true,
}: StStorageSceneProps) {
  const [keys] = useState(sceneKeys);
  const [addOpen, setAddOpen] = useState(initialDialog === "add");
  const [editKey, setEditKey] = useState<string | null>(initialDialog === "edit" ? (sceneKeys[0]?.key ?? null) : null);
  const [deleteKey, setDeleteKey] = useState<string | null>(null);
  const [clearOpen, setClearOpen] = useState(false);

  const hasKeys = useMemo(() => keys.length > 0, [keys]);

  const body = (
      <div className="ss">
        <StorageLayout realm={realm} position={position} active="scene">
          <div className="ss__section-header">
            <div className="ss__titlegroup">
              <h2 className="ss__title">Scene Storage</h2>
              {hasKeys ? <span className="ss__count">{keys.length} keys</span> : null}
            </div>
            <div className="ss__actions">
              <button type="button" className="ss-btn ss-btn--contained" onClick={() => setAddOpen(true)}>
                <span className="ss-btn__icon" aria-hidden="true"><AddIcon /></span>
                Add
              </button>
              {hasKeys ? (
                <button type="button" className="ss-btn ss-btn--outlined-error" onClick={() => setClearOpen(true)}>
                  <span className="ss-btn__icon" aria-hidden="true"><DeleteSweepIcon /></span>
                  Clear All
                </button>
              ) : null}
            </div>
          </div>

          {loading ? (
            <div className="ss__loading">
              <Spinner />
            </div>
          ) : (
            <KeyTable
              keys={keys}
              emptyLabel="No scene values found"
              onEdit={setEditKey}
              onDelete={setDeleteKey}
            />
          )}
        </StorageLayout>

        {addOpen ? (
          <SceneValueDialog mode="add" onClose={() => setAddOpen(false)} onSave={() => setAddOpen(false)} portal={portal} />
        ) : null}
        {editKey ? (
          <SceneValueDialog
            mode="edit"
            keyName={editKey}
            onClose={() => setEditKey(null)}
            onSave={() => setEditKey(null)}
            portal={portal}
          />
        ) : null}
        {deleteKey ? (
          <ConfirmDialog
            title="Delete Scene Value"
            message={`Are you sure you want to delete the scene value "${deleteKey}"? This action cannot be undone.`}
            onConfirm={() => setDeleteKey(null)}
            onCancel={() => setDeleteKey(null)}
            portal={portal}
          />
        ) : null}
        {clearOpen ? (
          <ConfirmDialog
            title="Clear All Scene Values"
            message="Are you sure you want to delete ALL scene values? This action cannot be undone."
            onConfirm={() => setClearOpen(false)}
            onCancel={() => setClearOpen(false)}
            portal={portal}
          />
        ) : null}
      </div>
  );

  return <SitesChromeMaybe chrome={chrome && !embedded} active="play">{body}</SitesChromeMaybe>;
}
