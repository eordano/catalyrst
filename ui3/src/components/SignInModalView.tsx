import {
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from "react";

import Button from "../atoms/Button";
import Spinner from "../atoms/Spinner";
import {
  AppleIcon,
  DiscordIcon,
  EmailIcon,
  GoogleIcon,
  WalletIcon,
} from "../atoms/BrandIcons";
import { CameraIcon } from "../atoms/icons";
import Modal from "./Modal";

import "./signinmodalview.css";

type Step = "options" | "wallet" | "qr" | "code" | "shell-wait";

export type SignInSocialProvider = "google" | "apple" | "discord";

export type SignInWallet = { rdns: string; name: string; icon?: string };

export type SignInShellHandle = {
  identity: Promise<unknown>;
  cancel: () => void;
};

export type PhonePairSession = {
  qrDataUrl: string;
  uri: string;
  libreUri?: string;
  deepLinks?: { name: string; href: string }[];
  connected: Promise<unknown>;
  cancel: () => void;
};

const SOCIALS: {
  provider: SignInSocialProvider;
  label: string;
  Icon: (p: { size?: number }) => ReactNode;
}[] = [
  { provider: "google", label: "Google", Icon: GoogleIcon },
  { provider: "apple", label: "Apple", Icon: AppleIcon },
  { provider: "discord", label: "Discord", Icon: DiscordIcon },
];

export type SignInModalViewProps = {
  onClose: () => void;
  onSignedIn?: () => void;
  inAppAvailable: boolean;
  walletAvailable: boolean;
  wallets?: SignInWallet[];
  desktopShell: boolean;
  authError: string | null;
  onStartEmailSignIn: (email: string) => Promise<void>;
  onVerifyEmailSignIn: (email: string, code: string) => Promise<unknown>;
  onSocialSignIn: (provider: SignInSocialProvider) => void;
  onConnectWallet: (walletRdns?: string) => Promise<unknown>;
  onPhonePair?: () => Promise<PhonePairSession | null>;
  onBeginShellSignIn: () => SignInShellHandle;
};

export default function SignInModalView({
  onClose,
  onSignedIn,
  inAppAvailable,
  walletAvailable,
  wallets = [],
  desktopShell,
  authError,
  onStartEmailSignIn,
  onVerifyEmailSignIn,
  onSocialSignIn,
  onConnectWallet,
  onPhonePair,
  onBeginShellSignIn,
}: SignInModalViewProps) {
  const [step, setStep] = useState<Step>("options");
  const [email, setEmail] = useState("");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [pendingWallet, setPendingWallet] = useState<string | null>(null);
  const [qr, setQr] = useState<PhonePairSession | null>(null);
  const [qrError, setQrError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const qrRef = useRef<PhonePairSession | null>(null);

  const inApp = inAppAvailable;
  const shell = desktopShell;
  const shellRef = useRef<SignInShellHandle | null>(null);
  const [shellError, setShellError] = useState<string | null>(null);

  const injectedPath = walletAvailable || wallets.length > 0;
  const showWallet = !shell && (injectedPath || !!onPhonePair);

  useEffect(() => () => shellRef.current?.cancel(), []);
  useEffect(() => () => qr?.cancel(), [qr]);

  async function onShellBrowser() {
    if (busy || step === "shell-wait") return;
    setShellError(null);
    setStep("shell-wait");
    const handle = onBeginShellSignIn();
    shellRef.current = handle;
    const identity = await handle.identity;
    shellRef.current = null;
    if (identity) {
      onSignedIn?.();
      onClose();
    } else {
      setShellError("The browser didn't confirm the sign-in. Try again.");
      setStep("options");
    }
  }

  function onShellCancel() {
    shellRef.current?.cancel();
    shellRef.current = null;
    setStep("options");
  }

  async function onEmailContinue(e: FormEvent) {
    e.preventDefault();
    const value = email.trim();
    if (!value || busy) return;
    setBusy(true);
    try {
      await onStartEmailSignIn(value);
      setStep("code");
    } catch {
    } finally {
      setBusy(false);
    }
  }

  async function onVerify(e: FormEvent) {
    e.preventDefault();
    const value = code.trim();
    if (!value || busy) return;
    setBusy(true);
    try {
      const identity = await onVerifyEmailSignIn(email.trim(), value);
      if (identity) {
        onSignedIn?.();
        onClose();
      }
    } finally {
      setBusy(false);
    }
  }

  function onSocial(provider: SignInSocialProvider) {
    if (busy) return;
    onSocialSignIn(provider);
  }

  async function onWalletEntry() {
    if (busy) return;
    if (!injectedPath) {
      await startQr();
      return;
    }
    setStep("wallet");
  }

  async function onWallet(rdns?: string) {
    if (busy) return;
    setBusy(true);
    setPendingWallet(rdns ?? "injected");
    try {
      const identity = await onConnectWallet(rdns);
      if (identity) {
        onSignedIn?.();
        onClose();
      }
    } finally {
      setBusy(false);
      setPendingWallet(null);
    }
  }

  async function startQr() {
    setQrError(null);
    setCopied(false);
    setStep("qr");
    if (!onPhonePair) {
      setQrError("Phone sign-in isn't available right now.");
      return;
    }
    try {
      const session = await onPhonePair();
      if (!session) {
        setQrError("Phone sign-in isn't available right now.");
        return;
      }
      qrRef.current = session;
      setQr(session);
      const identity = await session.connected;
      if (identity) {
        onSignedIn?.();
        onClose();
      } else if (qrRef.current === session) {
        qrRef.current = null;
        setQr(null);
        setQrError("That code expired — try again.");
      }
    } catch {
      setQrError("Couldn't start phone sign-in. Try again.");
    }
  }

  function leaveQr() {
    qr?.cancel();
    qrRef.current = null;
    setQr(null);
    setQrError(null);
    setStep(injectedPath ? "wallet" : "options");
  }

  const emailValid = /.+@.+\..+/.test(email.trim());

  return (
    <Modal
      onClose={onClose}
      width={400}
      ariaLabel="Sign in to Decentraland"
      closeOnBackdrop={false}
    >
      <div className="signin">
        <h2 className="signin__title">Sign in or sign up</h2>

        {step === "options" && (
          <div className="signin__options">
            {shell ? (
              <>
                <Button onClick={onShellBrowser} disabled={busy}>
                  Continue in your browser
                </Button>
                <p className="signin__note">
                  Uses your existing Decentraland session — no need to sign in
                  again.
                </p>
                {shellError && (
                  <p className="signin__note" role="alert">
                    {shellError}
                  </p>
                )}
              </>
            ) : null}

            {inApp &&
              SOCIALS.map(({ provider, label, Icon }) => (
                <button
                  key={provider}
                  type="button"
                  className="signin__choice"
                  onClick={() => onSocial(provider)}
                  disabled={busy}
                >
                  <span className="signin__choiceicon" aria-hidden="true">
                    <Icon size={22} />
                  </span>
                  <span className="signin__choicelabel">
                    Continue with {label}
                  </span>
                </button>
              ))}

            {showWallet && (
              <button
                type="button"
                className="signin__choice"
                onClick={onWalletEntry}
                disabled={busy}
              >
                <span className="signin__choiceicon" aria-hidden="true">
                  <WalletIcon size={22} />
                </span>
                <span className="signin__choicelabel">Continue with wallet</span>
              </button>
            )}

            {!shell && !inApp && !showWallet && (
              <p className="signin__note">
                Sign-in is temporarily unavailable (no sign-in provider is
                configured). Please try again shortly.
              </p>
            )}

            {inApp && (
              <>
                <div className="signin__or">
                  <span>or with email</span>
                </div>
                <form className="signin__emailrow" onSubmit={onEmailContinue}>
                  <span className="signin__emailicon" aria-hidden="true">
                    <EmailIcon size={18} />
                  </span>
                  <input
                    className="signin__emailinput"
                    type="email"
                    autoComplete="email"
                    aria-label="Email"
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                    placeholder="you@example.com"
                    disabled={busy}
                  />
                  <button
                    type="submit"
                    className="signin__emailgo"
                    disabled={busy || !emailValid}
                    aria-label="Continue with email"
                  >
                    {busy && step === "options" ? (
                      <Spinner size={16} />
                    ) : (
                      "Continue"
                    )}
                  </button>
                </form>
              </>
            )}

          </div>
        )}

        {step === "wallet" && (
          <div className="signin__options">
            {wallets.length > 0 ? (
              wallets.map((w) => (
                <button
                  key={w.rdns}
                  type="button"
                  className="signin__choice"
                  onClick={() => onWallet(w.rdns)}
                  disabled={busy}
                >
                  <span className="signin__choiceicon" aria-hidden="true">
                    {w.icon ? (
                      <img src={w.icon} alt="" width={22} height={22} />
                    ) : (
                      <WalletIcon size={22} />
                    )}
                  </span>
                  <span className="signin__choicelabel">
                    Continue with {w.name}
                  </span>
                  {busy && pendingWallet === w.rdns ? <Spinner size={16} /> : null}
                </button>
              ))
            ) : walletAvailable ? (
              <button
                type="button"
                className="signin__choice"
                onClick={() => onWallet()}
                disabled={busy}
              >
                <span className="signin__choiceicon" aria-hidden="true">
                  <WalletIcon size={22} />
                </span>
                <span className="signin__choicelabel">Use a browser wallet</span>
                {busy && pendingWallet ? <Spinner size={16} /> : null}
              </button>
            ) : null}

            <div className="signin__or">
              <span>or scan with a phone</span>
            </div>
            <button
              type="button"
              className="signin__choice"
              onClick={startQr}
              disabled={busy}
            >
              <span className="signin__choiceicon" aria-hidden="true">
                <CameraIcon size={20} />
              </span>
              <span className="signin__choicelabel">
                Sign with the wallet on your phone
              </span>
            </button>

            <button
              type="button"
              className="signin__back"
              onClick={() => setStep("options")}
            >
              ← Back
            </button>
          </div>
        )}

        {step === "qr" && (
          <div className="signin__form">
            {qrError ? (
              <p className="signin__note" role="alert">
                {qrError}
              </p>
            ) : qr ? (
              <>
                <p className="signin__brand">
                  <span className="signin__brandname">LibreConnect</span>
                  <span className="signin__brandhint">
                    Open QR sign-in — no WalletConnect, no project ID
                  </span>
                </p>
                <p className="signin__sent">
                  Scan with your phone camera or wallet app — signing happens
                  in your wallet&apos;s browser.
                </p>
                <img
                  className="signin__qr"
                  src={qr.qrDataUrl}
                  alt="QR code linking to this site's phone sign-in page"
                />
                <p className="signin__qrurl" data-pair-url={qr.uri}>
                  {qr.uri}
                </p>
                <button
                  type="button"
                  className="signin__copylink"
                  onClick={() => {
                    void navigator.clipboard?.writeText(qr.uri).then(
                      () => setCopied(true),
                      () => setCopied(false),
                    );
                  }}
                >
                  {copied ? "Copied ✓" : "Copy link"}
                </button>
                {qr.deepLinks?.length ? (
                  <div className="signin__deeplinks">
                    <span className="signin__deeplinkslabel">
                      On your phone, open in:
                    </span>
                    <div className="signin__deeplinkrow">
                      {qr.deepLinks.map((w) => (
                        <a
                          key={w.name}
                          className="signin__deeplink"
                          href={w.href}
                          target="_blank"
                          rel="noreferrer"
                        >
                          {w.name}
                        </a>
                      ))}
                    </div>
                  </div>
                ) : null}
                {qr.libreUri ? (
                  // Tucked away: mainstream wallets (MetaMask & co.) can't
                  // parse libre: URIs — surfacing the raw URI invites pasting
                  // it into wallet scanners, which fails. The https QR/link
                  // above is the compatible path; this stays for
                  // LibreConnect-aware wallets only.
                  <details className="signin__advanced">
                    <summary>Advanced: LibreConnect URI</summary>
                    <p
                      className="signin__libreuri"
                      data-libre-uri={qr.libreUri}
                      title="LibreConnect pairing URI — only for wallets with native LibreConnect support"
                    >
                      {qr.libreUri}
                    </p>
                  </details>
                ) : null}
              </>
            ) : (
              <>
                <Spinner size={24} />
                <p className="signin__sent">Starting phone sign-in…</p>
              </>
            )}
            <button type="button" className="signin__back" onClick={leaveQr}>
              ← Back
            </button>
          </div>
        )}

        {step === "shell-wait" && (
          <div className="signin__form">
            <Spinner size={24} />
            <p className="signin__sent">
              Approve the request in your browser — we opened a Decentraland
              tab for you.
            </p>
            <button
              type="button"
              className="signin__back"
              onClick={onShellCancel}
            >
              ← Cancel
            </button>
          </div>
        )}

        {step === "code" && (
          <form className="signin__form" onSubmit={onVerify}>
            <p className="signin__sent">
              We sent a code to <strong>{email}</strong>.
            </p>
            <label className="signin__label" htmlFor="signin-code">
              Verification code
            </label>
            <input
              id="signin-code"
              className="signin__input"
              inputMode="numeric"
              autoComplete="one-time-code"
              autoFocus
              value={code}
              onChange={(e) => setCode(e.target.value)}
              placeholder="123456"
              disabled={busy}
            />
            <Button type="submit" disabled={busy || !code.trim()}>
              {busy ? <Spinner size={16} /> : "Verify & sign in"}
            </Button>
            <button
              type="button"
              className="signin__back"
              onClick={() => {
                setCode("");
                setStep("options");
              }}
              disabled={busy}
            >
              ← Use a different email
            </button>
          </form>
        )}

        {authError && (
          <p className="signin__error" role="alert">
            {authError}
          </p>
        )}
      </div>
    </Modal>
  );
}
