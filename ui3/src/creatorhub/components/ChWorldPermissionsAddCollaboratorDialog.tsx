import { useCallback, useState, type ChangeEvent } from "react";
import Button from "../../atoms/Button";
import ChDialogShell from "./ChDialogShell";
import "./chworldpermissionsaddcollaboratordialog.css";
import { isValidAddress } from "../../data/format";


const WORLD_NAME = "mystore.dcl.eth";
const INVALID_ADDRESS = "Invalid address";

const LockIcon = ({ size = 20 }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <rect x="5" y="10.5" width="14" height="10" rx="2" fill="currentColor" />
    <path d="M8 10.5V8a4 4 0 0 1 8 0v2.5" stroke="currentColor" strokeWidth="2" fill="none" strokeLinecap="round" />
  </svg>
);

type AddCollaboratorFormProps = {
  value?: string;
  error?: string | null;
  onClose?: () => void;
};

function AddCollaboratorForm({ value = "", error = null, onClose = () => {} }: AddCollaboratorFormProps) {
  const [address, setAddress] = useState(value);
  const [err, setErr] = useState<string | null>(error);

  const handleChange = useCallback((e: ChangeEvent<HTMLInputElement>) => {
    setAddress(e.target.value);
    setErr(null);
  }, []);

  const handleSubmit = useCallback(() => {
    if (!address || !isValidAddress(address)) {
      setErr(INVALID_ADDRESS);
      return;
    }
    setErr(null);
  }, [address]);

  const isValid = address.length > 0 && !err;

  return (
    <div className="acd__form">
      <h5 className="acd__title">Add Collaborator</h5>

      <div className={"acd__field" + (err ? " is-error" : "")}>
        <input
          className="acd__input"
          type="text"
          placeholder="0x..."
          autoFocus
          value={address}
          onChange={handleChange}
          aria-invalid={!!err}
          aria-label="Wallet address"
        />
        {err && <p className="acd__helper">{err}</p>}
      </div>

      <div className="acd__actions">
        <Button variant="secondary" onClick={onClose}>Cancel</Button>
        <Button variant="primary" disabled={!isValid} onClick={handleSubmit}>
          Confirm
        </Button>
      </div>
    </div>
  );
}

const TABS = [
  { value: "access", label: "Access" },
  { value: "collaborators", label: "Collaborators" },
];

export default function ChWorldPermissionsAddCollaboratorDialog({
  variant = "modal" as "modal" | "panel",
  value = "",
  error = null,
  chrome = true,
  onClose = () => {},
}) {
  const form = <AddCollaboratorForm value={value} error={error} onClose={onClose} />;
  if (!chrome) {
    return (
      <div
        className={
          "acd__backdrop" + (variant === "panel" ? " acd__backdrop--panel" : "")
        }
      >
        {form}
      </div>
    );
  }
  return (
    <ChDialogShell
      variant={variant}
      className="acd"
      icon={<LockIcon />}
      title={`Permissions - ${WORLD_NAME}`}
      ariaLabel="Add collaborator"
      onClose={onClose}
      tabs={TABS}
      activeTab="collaborators"
    >
      <div className="acd__centered">{form}</div>
    </ChDialogShell>
  );
}
