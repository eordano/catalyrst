import AccountChrome from "../frames/AccountChrome";
import StarWalletIcon from "../../atoms/StarWalletIcon";
import "./acsignin.css";

const DesktopMessage = () => (
  <span>
    You can use the{" "}
    <a href="https://metamask.io" target="_blank" rel="noopener noreferrer">
      MetaMask
    </a>{" "}
    extension or your email account
  </span>
);

export default function AcSignIn({
  isConnecting = false,
  isConnected = false,
  hasError = false,
}: {
  isConnecting?: boolean;
  isConnected?: boolean;
  hasError?: boolean;
}) {
  const label = isConnecting
    ? "Connecting..."
    : isConnected
    ? "Connected"
    : "Connect";
  const disabled = isConnecting || isConnected;

  const errorVisible = hasError && !isConnecting && !isConnected;

  return (
    <AccountChrome>
      <div className="acsignin">
        <div className="acsignin__card">
          <h1 className="acsignin__header">Get Started</h1>

          <div className="acsignin__icon">
            <StarWalletIcon />
          </div>

          <p className="acsignin__message">
            <DesktopMessage />
          </p>

          <button
            type="button"
            className="acsignin__button"
            disabled={disabled}
          >
            {label}
          </button>

          <p
            className={
              "acsignin__error" + (errorVisible ? " is-visible" : "")
            }
          >
            Could not connect to wallet.
          </p>

          {isConnecting && (
            <div className="acsignin__redirect" role="status">
              <span className="acsignin__spinner" aria-hidden="true" />
              Redirecting you to the login flow…
            </div>
          )}
        </div>
      </div>
    </AccountChrome>
  );
}
