import { siteUrl } from "../../data/site";
import { Suspense, lazy, useState } from "react";
import { Avatar } from "../../atoms/primitives";
import { Close } from "../../atoms/icons";
import "./profilewidget.css";

const SignInFlow = lazy(() => import("../../overlay/SignInFlow"));

function openExternal(url: string) {
  if (typeof window !== "undefined")
    window.open(url, "_blank", "noopener,noreferrer");
}

type ProfileWidgetProps = {
  open?: boolean;
  name?: string;
  tag?: string;
  wallet?: string;
  address?: string;
  avatarSrc?: string | null;
  isGuest?: boolean;
  /** Dismiss the popover without leaving the world/menu (Close). */
  onClose?: () => void;
  /** Ends the session (bridge `Logout`) and dismisses the popover. Omit to keep
   *  the SIGN OUT action disabled (e.g. in Storybook without a live bridge). */
  onSignOut?: () => void;
  /** Positions the popover near the world rail's profile button (default) or the
   *  Explore top-bar's profile chip. */
  anchor?: "rail" | "topbar";
};

export default function ProfileWidget({
  open = false,
  name = "Guest",
  tag = "",
  wallet = "",
  address,
  avatarSrc = null,
  isGuest = false,
  onClose,
  onSignOut,
  anchor = "rail",
}: ProfileWidgetProps) {
  const [signInOpen, setSignInOpen] = useState(false);
  function copyAddress() {
    const value = address || wallet;
    if (!value) return;
    try {
      navigator.clipboard?.writeText(value);
    } catch {
    }
  }
  if (!open) return null;
  return (
    <>
    <div className="pw__stage">
      <div className={"pw" + (anchor === "topbar" ? " pw--topbar" : "")}>
        <div className="pw__menu">
          <div className="pw__menuhead">
            <Avatar
              size={72}
              src={avatarSrc || undefined}
              name={name}
              seed={address || name}
              alt={name || "avatar"}
              className="pw__avatar"
            />
            <div className="pw__name">
              <span className="pw__namemain">{name || "Guest"}</span>
              {tag ? <span className="pw__tag">({tag})</span> : null}
            </div>
            {wallet ? (
              <>
                <div className="pw__walletlabel">WALLET ADDRESS</div>
                <button type="button" className="u-wallet" title="Copy address" onClick={copyAddress}>
                  {wallet}
                  <span className="pw__copy" aria-hidden="true">⎘</span>
                </button>
              </>
            ) : null}
          </div>

          <button className="pw__profile" data-sb-linkto="Explorer/Pages/Passport">VIEW PROFILE</button>

          {isGuest && (
            <button
              type="button"
              className="pw__item pw__item--connect"
              onClick={() => setSignInOpen(true)}
            >
              <span className="pw__icon" aria-hidden="true">⛓</span>
              Sign in
            </button>
          )}

          <div className="pw__actions">
            <button type="button" className="pw__item" onClick={() => onClose?.()}>
              <span className="pw__icon" aria-hidden="true">
                <Close size={16} />
              </span>
              CLOSE
            </button>
            <button
              type="button"
              className="pw__item pw__item--exit"
              aria-disabled={onSignOut ? undefined : "true"}
              title={onSignOut ? undefined : "Sign out isn't available in this build yet"}
              onClick={onSignOut ? () => onSignOut() : (e) => e.preventDefault()}
            >
              <span className="pw__icon" aria-hidden="true">
                <svg viewBox="0 0 24 24" width="16" height="16" fill="none"
                     stroke="currentColor" strokeWidth="2"
                     strokeLinecap="round" strokeLinejoin="round">
                  <path d="M12 3v8" />
                  <path d="M6.6 6.8a7 7 0 1 0 10.8 0" />
                </svg>
              </span>
              SIGN OUT
            </button>
          </div>

          <div className="pw__footer">
            <button
              className="pw__legal"
              type="button"
              onClick={() => openExternal(siteUrl("/terms"))}
            >
              Terms of Service
            </button>
            <button
              className="pw__legal"
              type="button"
              onClick={() => openExternal(siteUrl("/privacy"))}
            >
              Privacy Policy
            </button>
          </div>
        </div>
      </div>
    </div>
    {signInOpen && (
      <Suspense fallback={null}>
        <SignInFlow onClose={() => setSignInOpen(false)} />
      </Suspense>
    )}
    </>
  );
}
