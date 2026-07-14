import { useCallback, useId, useMemo, useState, type ChangeEvent } from "react";
import Button from "../../atoms/Button";
import ChDialogShell from "./ChDialogShell";
import "./chworldpermissionssetchangepassworddialo.css";

const MIN_PASSWORD_LENGTH = 8;
const MIN_NUMBERS = 2;

function countDigits(str: string) {
  return (str.match(/\d/g) ?? []).length;
}

const LockIcon = ({ size = 20 }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <rect x="5" y="10.5" width="14" height="10" rx="2" fill="currentColor" />
    <path d="M8 10.5V8a4 4 0 0 1 8 0v2.5" stroke="currentColor" strokeWidth="2" fill="none" strokeLinecap="round" />
  </svg>
);
const InfoOutlinedIcon = ({ size = 20 }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="1.8" fill="none" />
    <path d="M12 11v5M12 7.6v.2" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
  </svg>
);
const VisibilityIcon = ({ size = 20 }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <path d="M12 5c-5 0-9.27 3.11-11 7 1.73 3.89 6 7 11 7s9.27-3.11 11-7c-1.73-3.89-6-7-11-7Z" stroke="currentColor" strokeWidth="1.6" fill="none" />
    <circle cx="12" cy="12" r="3" stroke="currentColor" strokeWidth="1.6" fill="none" />
  </svg>
);
const VisibilityOffIcon = ({ size = 20 }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <path d="M12 5c-5 0-9.27 3.11-11 7a12.3 12.3 0 0 0 4 4.7M9.5 5.3A11.6 11.6 0 0 1 12 5c5 0 9.27 3.11 11 7a12.4 12.4 0 0 1-4.2 4.85" stroke="currentColor" strokeWidth="1.6" fill="none" strokeLinecap="round" />
    <path d="M9.9 9.9a3 3 0 0 0 4.2 4.2" stroke="currentColor" strokeWidth="1.6" fill="none" strokeLinecap="round" />
    <path d="M4 4l16 16" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
  </svg>
);

const WORLD_NAME = "mystore.dcl.eth";

type PasswordFormProps = {
  isChanging: boolean;
  initialPassword?: string;
  initialConfirm?: string;
  onClose?: () => void;
  onSubmit?: (password: string) => void;
};

function PasswordForm({
  isChanging,
  initialPassword = "",
  initialConfirm = "",
  onClose = () => {},
  onSubmit = () => {},
}: PasswordFormProps) {
  const uid = useId();
  const pwId = `${uid}-pw`;
  const repeatId = `${uid}-repeat`;
  const [password, setPassword] = useState(initialPassword);
  const [confirmPassword, setConfirmPassword] = useState(initialConfirm);
  const [showPassword, setShowPassword] = useState(false);
  const [showConfirmPassword, setShowConfirmPassword] = useState(false);

  const handlePasswordChange = useCallback(
    (e: ChangeEvent<HTMLInputElement>) => setPassword(e.target.value),
    [],
  );
  const handleConfirmPasswordChange = useCallback(
    (e: ChangeEvent<HTMLInputElement>) => setConfirmPassword(e.target.value),
    [],
  );
  const togglePassword = useCallback(() => setShowPassword((p) => !p), []);
  const toggleConfirm = useCallback(() => setShowConfirmPassword((p) => !p), []);

  const hasMinLength = password.length >= MIN_PASSWORD_LENGTH;
  const hasMinNumbers = countDigits(password) >= MIN_NUMBERS;
  const passwordMeetsRequirements = hasMinLength && hasMinNumbers;
  const isValid = passwordMeetsRequirements && password === confirmPassword;
  const showMismatchError =
    confirmPassword.length > 0 && password.length > 0 && password !== confirmPassword;

  const passwordErrors = useMemo(() => {
    if (password.length === 0) return null;
    if (passwordMeetsRequirements) return null;
    const failed = [];
    if (!hasMinLength) failed.push(`Minimum length: ${MIN_PASSWORD_LENGTH} characters`);
    if (!hasMinNumbers) failed.push(`At least ${MIN_NUMBERS} numbers`);
    return failed;
  }, [password, passwordMeetsRequirements, hasMinLength, hasMinNumbers]);

  return (
    <div className="cpw__form">
      <h5 className="cpw__title">
        {isChanging ? "Change Password" : "Create New Password"}
      </h5>

      <div className="cpw__field">
        <label className="cpw__label" htmlFor={pwId}>Type your password</label>
        <div className={"cpw__input" + (passwordErrors ? " is-error" : "")}>
          <input
            id={pwId}
            type={showPassword ? "text" : "password"}
            value={password}
            onChange={handlePasswordChange}
            autoFocus
          />
          <button
            type="button"
            className="cpw__adornment"
            aria-label={showPassword ? "Hide password" : "Show password"}
            onClick={togglePassword}
          >
            {showPassword ? <VisibilityOffIcon /> : <VisibilityIcon />}
          </button>
        </div>
        {passwordErrors && (
          <div className="cpw__requirements">
            <span className="cpw__reqheader">Password must include:</span>
            <ul>
              {passwordErrors.map((err) => (
                <li key={err}>{err}</li>
              ))}
            </ul>
          </div>
        )}
      </div>

      <div className="cpw__field">
        <label className="cpw__label" htmlFor={repeatId}>Repeat your password</label>
        <div className={"cpw__input" + (showMismatchError ? " is-error" : "")}>
          <input
            id={repeatId}
            type={showConfirmPassword ? "text" : "password"}
            value={confirmPassword}
            onChange={handleConfirmPasswordChange}
          />
          <button
            type="button"
            className="cpw__adornment"
            aria-label={showConfirmPassword ? "Hide password" : "Show password"}
            onClick={toggleConfirm}
          >
            {showConfirmPassword ? <VisibilityOffIcon /> : <VisibilityIcon />}
          </button>
        </div>
        {showMismatchError && (
          <span className="cpw__helpererror">Passwords do not match</span>
        )}
      </div>

      <div className="cpw__info">
        <InfoOutlinedIcon size={20} />
        <span>Make sure to write down your password so you don't lose it!</span>
      </div>

      <div className="cpw__actions">
        <Button variant="secondary" onClick={onClose}>Cancel</Button>
        <Button
          variant="primary"
          disabled={!isValid}
          onClick={() => onSubmit(password)}
        >
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

export default function ChWorldPermissionsSetChangePasswordDialo({
  variant = "modal" as "modal" | "panel",
  isChanging = false,
  initialPassword = "",
  initialConfirm = "",
  onClose = () => {},
  onSubmit = (_password: string) => {},
}) {
  return (
    <ChDialogShell
      variant={variant}
      className="cpw"
      icon={<LockIcon />}
      title={`Permissions - ${WORLD_NAME}`}
      ariaLabel={"World permissions \u{2014} set password"}
      onClose={onClose}
      tabs={TABS}
      activeTab="access"
    >
      <div className="cpw__centered">
        <PasswordForm
          isChanging={isChanging}
          initialPassword={initialPassword}
          initialConfirm={initialConfirm}
          onClose={onClose}
          onSubmit={onSubmit}
        />
      </div>
    </ChDialogShell>
  );
}
