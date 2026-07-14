import type { CSSProperties } from "react";
import { useMemo, useState } from "react";
import SearchField from "../../atoms/SearchField";
import { SitesChromeMaybe } from "../frames/SitesChrome";
import Modal from "../../components/Modal";
import "./stwhatsonadminusers.css";
import { truncateAddress } from "../../data/format";

const COLUMNS = [
  { key: "approve_own_event", label: "Approve Own Hangouts", modalDesc: "Allow this user to approve hangouts they create" },
  { key: "approve_any_event", label: "Approve Hangouts", modalDesc: "Allow this user to approve any hangout" },
  { key: "edit_any_event", label: "Edit Hangouts", modalDesc: "Allow this user to edit any hangout" },
  { key: "edit_any_profile", label: "Edit Users", modalDesc: "Allow this user to manage admin permissions" },
];

export type UserRow = {
  user: string;
  name: string | null;
  permissions: string[];
  hue: number;
};


const CheckIcon = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor" aria-hidden="true">
    <path d="M9 16.17 4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41L9 16.17Z" />
  </svg>
);
const CloseIcon = () => (
  <svg viewBox="0 0 24 24" width="22" height="22" fill="currentColor" aria-hidden="true">
    <path d="M19 6.41 17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12 19 6.41Z" />
  </svg>
);
const ChevronLeft = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor" aria-hidden="true">
    <path d="M15.41 7.41 14 6l-6 6 6 6 1.41-1.41L10.83 12l4.58-4.59Z" />
  </svg>
);
const ChevronRight = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor" aria-hidden="true">
    <path d="M8.59 16.59 13.17 12 8.59 7.41 10 6l6 6-6 6-1.41-1.41Z" />
  </svg>
);
const CaretDown = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
    <path d="M7 10l5 5 5-5H7Z" />
  </svg>
);

const ADMIN_TABS = [
  { id: "whats_on", label: "What's On" },
  { id: "pending", label: "Pending Hangouts" },
  { id: "users", label: "Users" },
];

function AdminTabsBar({ active = "users" }: { active?: string }) {
  return (
    <div className="au-bar">
      <div className="au-bar__tabs" role="tablist">
        {ADMIN_TABS.map((tab) => (
          <button
            key={tab.id}
            type="button"
            role="tab"
            aria-selected={tab.id === active}
            className={"au-bar__tab" + (tab.id === active ? " is-active" : "")}
          >
            {tab.label}
          </button>
        ))}
      </div>
      <div className="au-bar__cta">
        <button type="button" className="au-bar__create">+ Create Hangout</button>
      </div>
    </div>
  );
}

function AdminPermissionsModal({
  mode,
  user,
  hue,
  initialPermissions,
  isSubmitting,
  onClose,
  onSubmit,
}: {
  mode: "add" | "edit";
  user?: string;
  hue?: number;
  initialPermissions: string[];
  isSubmitting?: boolean;
  onClose?: () => void;
  onSubmit: (payload: { address: string; permissions: string[] }) => void;
}) {
  const [address, setAddress] = useState(mode === "edit" ? user ?? "" : "");
  const [permissions, setPermissions] = useState(initialPermissions);

  const addressIsValid = /^0x[a-fA-F0-9]{40}$/.test(address.trim());
  const addressHasInvalidFormat = address.length > 0 && !addressIsValid;
  const canSave = addressIsValid && !isSubmitting;

  const toggle = (key: string) =>
    setPermissions((prev) => (prev.includes(key) ? prev.filter((v) => v !== key) : [...prev, key]));

  return (
    <Modal onClose={onClose} width="100%" className="modal__card--plain au-modal" ariaLabel={mode === "edit" ? "Edit User" : "Add User"}>
        <div className="au-modal__title">
          <span>{mode === "edit" ? "Edit User" : "Add User"}</span>
          <button type="button" className="au-modal__close" aria-label="close" onClick={onClose}>
            <CloseIcon />
          </button>
        </div>

        {mode === "edit" && user && (
          <div className="au-modal__header">
            <span className="au-modal__avatar u-avatar" style={{ "--sz": "56px", "--hue": hue } as CSSProperties} aria-hidden="true" />
            <div className="au-modal__headertext">
              <span className="au-modal__name">{truncateAddress(user)}</span>
              <span className="au-modal__address u-truncate">{user}</span>
            </div>
          </div>
        )}

        <div className="au-modal__content">
          {mode === "add" && (
            <div className="au-field">
              <label className="au-field__label" htmlFor="au-wallet">Wallet Address</label>
              <input
                id="au-wallet"
                className={"au-field__input" + (addressHasInvalidFormat ? " is-error" : "")}
                placeholder={"0x\u{2026}"}
                value={address}
                onChange={(e) => setAddress(e.target.value)}
                aria-label="Wallet Address"
              />
              <span className={"au-field__help" + (addressHasInvalidFormat ? " is-error" : "")}>
                {addressHasInvalidFormat ? "Enter a valid Ethereum address" : " "}
              </span>
            </div>
          )}

          <div className="au-perms">
            {COLUMNS.map((col) => {
              const on = permissions.includes(col.key);
              return (
                <div className="au-perm" key={col.key}>
                  <div className="au-perm__meta">
                    <span className="au-perm__title">{col.label}</span>
                    <span className="au-perm__desc">{col.modalDesc}</span>
                  </div>
                  <button
                    type="button"
                    role="switch"
                    aria-checked={on}
                    aria-label={col.label}
                    className={"au-switch" + (on ? " is-on" : "")}
                    onClick={() => toggle(col.key)}
                  >
                    <span className="au-switch__track" />
                    <span className="au-switch__thumb" />
                  </button>
                </div>
              );
            })}
          </div>
        </div>

        <div className="au-modal__footer">
          <button type="button" className="au-btn au-btn--secondary" onClick={onClose} disabled={isSubmitting}>
            Cancel
          </button>
          <button
            type="button"
            className="au-btn au-btn--primary"
            onClick={() => onSubmit({ address: address.trim(), permissions })}
            disabled={!canSave}
          >
            Save Changes
          </button>
        </div>
    </Modal>
  );
}

function UserTableRow({ row, onClick }: { row: UserRow; onClick?: () => void }) {
  return (
    <tr className="au-row" onClick={onClick}>
      <td className="au-cell au-cell--user">
        <span className="au-avatar u-avatar" style={{ "--sz": "40px", "--hue": row.hue } as CSSProperties} aria-hidden="true" />
        <span className="au-cell__addr">{row.user}</span>
        {row.name ? <span className="au-cell__name">{` (${row.name})`}</span> : null}
      </td>
      {COLUMNS.map((col) => (
        <td key={col.key} className="au-cell au-cell--center">
          {row.permissions.includes(col.key) ? (
            <span className="au-check" role="img" aria-label="enabled">
              <CheckIcon />
            </span>
          ) : null}
        </td>
      ))}
    </tr>
  );
}

type Feedback = { message: string; severity: string };

type ModalState = { mode: "add" | "edit"; user?: string; hue?: number; permissions: string[] };

type StWhatSOnAdminUsersProps = {
  chrome?: boolean;
  users?: UserRow[];
  loading?: boolean;
  initialFeedback?: Feedback | null;
};

export default function StWhatSOnAdminUsers({
  chrome = true,
  users = [],
  loading = false,
  initialFeedback = null,
}: StWhatSOnAdminUsersProps) {
  const [modalState, setModalState] = useState<ModalState | null>(null);
  const [feedback, setFeedback] = useState<Feedback | null>(initialFeedback);
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);
  const [rowsPerPage] = useState(10);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return users;
    return users.filter((row) => {
      if (row.user.toLowerCase().includes(q)) return true;
      return row.name ? row.name.toLowerCase().includes(q) : false;
    });
  }, [users, search]);

  const paginated = useMemo(
    () => filtered.slice(page * rowsPerPage, page * rowsPerPage + rowsPerPage),
    [filtered, page, rowsPerPage]
  );

  const from = filtered.length === 0 ? 0 : page * rowsPerPage + 1;
  const to = Math.min(filtered.length, page * rowsPerPage + rowsPerPage);
  const lastPage = Math.max(0, Math.ceil(filtered.length / rowsPerPage) - 1);

  const handleSubmit = () => {
    setModalState(null);
    setFeedback({ message: "Permissions updated", severity: "success" });
  };

  return (
    <SitesChromeMaybe chrome={chrome} active="play">
      <div className="au">
        <AdminTabsBar active="users" />

        <div className="au__container">
          <h1 className="au__title">Users</h1>

          <div className="au__header">
            <div className="au__searchwrap">
              <SearchField
                placeholder="Type wallet address"
                value={search}
                onChange={(v) => {
                  setSearch(v);
                  setPage(0);
                }}
              />
            </div>
            <button
              type="button"
              className="au__adduser"
              onClick={() => setModalState({ mode: "add", permissions: [] })}
            >
              + Add User
            </button>
          </div>

          <div className="au__tablewrap">
            <table className="au__table" aria-label="Users">
              <thead>
                <tr>
                  <th className="au__th">User</th>
                  {COLUMNS.map((col) => (
                    <th key={col.key} className="au__th au__th--center">{col.label}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {paginated.map((row) => (
                  <UserTableRow
                    key={row.user}
                    row={row}
                    onClick={() => setModalState({ mode: "edit", user: row.user, hue: row.hue, permissions: row.permissions })}
                  />
                ))}
                {!loading && paginated.length === 0 && (
                  <tr>
                    <td className="au-cell au-cell--center au-cell--empty" colSpan={COLUMNS.length + 1}>
                      No admins configured
                    </td>
                  </tr>
                )}
              </tbody>
            </table>

            <div className="au__pagination">
              <span className="au__pglabel">Rows per page:</span>
              <span className="au__pgselect">
                {rowsPerPage}
                <CaretDown />
              </span>
              <span className="au__pgcount">
                {from}&#x2013;{to} of {filtered.length}
              </span>
              <button
                type="button"
                className="au__pgbtn"
                aria-label="Go to previous page"
                disabled={page === 0}
                onClick={() => setPage((p) => Math.max(0, p - 1))}
              >
                <ChevronLeft />
              </button>
              <button
                type="button"
                className="au__pgbtn"
                aria-label="Go to next page"
                disabled={page >= lastPage}
                onClick={() => setPage((p) => Math.min(lastPage, p + 1))}
              >
                <ChevronRight />
              </button>
            </div>
          </div>
        </div>

        {modalState && (
          <AdminPermissionsModal
            mode={modalState.mode}
            user={modalState.user}
            hue={modalState.hue}
            initialPermissions={modalState.permissions}
            isSubmitting={false}
            onClose={() => setModalState(null)}
            onSubmit={handleSubmit}
          />
        )}

        {feedback && (
          <div className="au-snack" role="status" aria-live="polite">
            <div className={"au-alert au-alert--" + feedback.severity}>
              <span className="au-alert__icon" aria-hidden="true">
                <CheckIcon />
              </span>
              <span className="au-alert__msg">{feedback.message}</span>
              <button type="button" className="au-alert__close" aria-label="Close" onClick={() => setFeedback(null)}>
                <CloseIcon />
              </button>
            </div>
          </div>
        )}
      </div>
    </SitesChromeMaybe>
  );
}
