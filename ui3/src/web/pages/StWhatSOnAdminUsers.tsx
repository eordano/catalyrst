import type { ReactNode } from "react";
import { useMemo, useState } from "react";
import Button from "../../atoms/Button";
import SearchField from "../../atoms/SearchField";
import Toggle from "../../atoms/Toggle";
import { Avatar } from "../../atoms/primitives";
import { Check, ChevronLeft, ChevronRight, Close } from "../../atoms/icons";
import Modal from "../../components/Modal";
import { SitesChromeMaybe } from "../frames/SitesChrome";
import StWhatSOnAdminTabs from "./StWhatSOnAdminTabs";
import { truncateAddress } from "../../data/format";
import "../../admin/admin.css";
import type { LabelSuffixProps } from "../../components/labelSuffix";

const COLUMNS = [
  { key: "approve_own_event", label: "Approve Own Hangouts", modalDesc: "Allow this user to approve hangouts they create" },
  { key: "approve_any_event", label: "Approve Hangouts", modalDesc: "Allow this user to approve any hangout" },
  { key: "edit_any_event", label: "Edit Hangouts", modalDesc: "Allow this user to edit any hangout" },
  { key: "edit_any_profile", label: "Edit Users", modalDesc: "Allow this user to manage admin permissions" },
];

const ROWS_PER_PAGE = 10;

const TONE: Record<string, "ok" | "bad" | undefined> = { success: "ok", error: "bad" };

export type UserRow = {
  user: string;
  name: string | null;
  permissions: string[];
  hue: number;
};

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
  const title = mode === "edit" ? "Edit User" : "Add User";

  const toggle = (key: string) =>
    setPermissions((prev) => (prev.includes(key) ? prev.filter((v) => v !== key) : [...prev, key]));

  return (
    <Modal onClose={onClose} width={520} ariaLabel={title}>
      <div className="adm__inner">
        <h2 className="adm__h2">{title}</h2>

        {mode === "edit" && user && (
          <div className="adm-card__head">
            <Avatar hue={hue} size={56} />
            <div>
              <div className="adm-card__title">{truncateAddress(user)}</div>
              <div className="adm-mono adm-dim u-truncate">{user}</div>
            </div>
          </div>
        )}

        {mode === "add" && (
          <div className={"adm-field" + (addressHasInvalidFormat ? " is-error" : "")}>
            <label className="adm-field__label" htmlFor="wallet-address">Wallet Address</label>
            <input
              id="wallet-address"
              className="adm-input"
              placeholder={"0x\u{2026}"}
              value={address}
              onChange={(e) => setAddress(e.target.value)}
              aria-label="Wallet Address"
            />
            <span className={"adm-field__help" + (addressHasInvalidFormat ? " is-error" : "")}>
              {addressHasInvalidFormat ? "Enter a valid Ethereum address" : " "}
            </span>
          </div>
        )}

        <ul className="adm-list">
          {COLUMNS.map((col) => (
            <li className="adm-actions adm-actions--split" key={col.key}>
              <div className="adm-field">
                <span className="adm-field__label">{col.label}</span>
                <span className="adm-field__help">{col.modalDesc}</span>
              </div>
              <Toggle checked={permissions.includes(col.key)} onChange={() => toggle(col.key)} ariaLabel={col.label} />
            </li>
          ))}
        </ul>

        <div className="adm-actions">
          <Button variant="secondary" onClick={onClose} disabled={isSubmitting}>
            Cancel
          </Button>
          <Button onClick={() => onSubmit({ address: address.trim(), permissions })} disabled={!canSave}>
            Save Changes
          </Button>
        </div>
      </div>
    </Modal>
  );
}

function UserTableRow({ row, onClick }: { row: UserRow; onClick?: () => void }) {
  return (
    <tr className="is-link" onClick={onClick}>
      <td>
        <Avatar hue={row.hue} size={40} />
        <span>{row.user}</span>
        {row.name ? <span className="adm-dim">{` (${row.name})`}</span> : null}
      </td>
      {COLUMNS.map((col) => (
        <td key={col.key} className="is-center">
          {row.permissions.includes(col.key) ? (
            <span className="adm-ok" role="img" aria-label="enabled">
              <Check size={20} />
            </span>
          ) : null}
        </td>
      ))}
    </tr>
  );
}

type Feedback = { message: string; severity: string };

type ModalState = { mode: "add" | "edit"; user?: string; hue?: number; permissions: string[] };

type StWhatSOnAdminUsersProps = LabelSuffixProps & {
  chrome?: boolean;
  users?: UserRow[];
  loading?: boolean;
  initialFeedback?: Feedback | null;
  nav?: ReactNode;
};

export default function StWhatSOnAdminUsers({
  labelSuffix,
  chrome = true,
  users = [],
  loading = false,
  initialFeedback = null,
  nav,
}: StWhatSOnAdminUsersProps) {
  const [modalState, setModalState] = useState<ModalState | null>(null);
  const [feedback, setFeedback] = useState<Feedback | null>(initialFeedback);
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return users;
    return users.filter((row) => {
      if (row.user.toLowerCase().includes(q)) return true;
      return row.name ? row.name.toLowerCase().includes(q) : false;
    });
  }, [users, search]);

  const paginated = useMemo(
    () => filtered.slice(page * ROWS_PER_PAGE, page * ROWS_PER_PAGE + ROWS_PER_PAGE),
    [filtered, page]
  );

  const from = filtered.length === 0 ? 0 : page * ROWS_PER_PAGE + 1;
  const to = Math.min(filtered.length, page * ROWS_PER_PAGE + ROWS_PER_PAGE);
  const lastPage = Math.max(0, Math.ceil(filtered.length / ROWS_PER_PAGE) - 1);

  const handleSubmit = () => {
    setModalState(null);
    setFeedback({ message: "Permissions updated", severity: "success" });
  };

  return (
    <SitesChromeMaybe chrome={chrome} active="play">
      <div className="adm">
        {nav ? <nav className="adm__nav" aria-label="Admin consoles">{nav}</nav> : null}
        <StWhatSOnAdminTabs active="users" labelSuffix={labelSuffix} />

        <div className="adm__page">
          <div className="adm__inner">
            <div className="adm__head">
              <h1 className="adm__title">Users</h1>
              <div className="adm__tools">
                <SearchField
                  placeholder="Type wallet address"
                  value={search}
                  onChange={(v) => {
                    setSearch(v);
                    setPage(0);
                  }}
                />
                <Button onClick={() => setModalState({ mode: "add", permissions: [] })}>+ Add User</Button>
              </div>
            </div>

            <div className="adm-scroll">
              <table className="adm-table" aria-label="Users">
                <thead>
                  <tr>
                    <th>User</th>
                    {COLUMNS.map((col) => (
                      <th key={col.key} className="is-center">{col.label}</th>
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
                      <td className="is-empty" colSpan={COLUMNS.length + 1}>
                        No admins configured
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>

            <div className="adm-actions">
              <span className="adm-dim">Rows per page: {ROWS_PER_PAGE}</span>
              <span className="adm-dim">
                {from}&#x2013;{to} of {filtered.length}
              </span>
              <Button
                variant="ghost"
                size="sm"
                aria-label="Go to previous page"
                disabled={page === 0}
                onClick={() => setPage((p) => Math.max(0, p - 1))}
              >
                <ChevronLeft />
              </Button>
              <Button
                variant="ghost"
                size="sm"
                aria-label="Go to next page"
                disabled={page >= lastPage}
                onClick={() => setPage((p) => Math.min(lastPage, p + 1))}
              >
                <ChevronRight />
              </Button>
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
          <div className="adm-toast" data-tone={TONE[feedback.severity]} role="status" aria-live="polite">
            <span aria-hidden="true">{TONE[feedback.severity] === "bad" ? <Close size={16} /> : <Check />}</span>
            <span className="adm-toast__msg">{feedback.message}</span>
            <Button variant="ghost" size="sm" aria-label="Close" onClick={() => setFeedback(null)}>
              <Close size={16} />
            </Button>
          </div>
        )}
      </div>
    </SitesChromeMaybe>
  );
}
