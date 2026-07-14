import { siteUrl } from "../../data/site";
import { Suspense, lazy, useEffect, useRef, useState } from "react";
import AuthLayout from "../../web/frames/AuthLayout";
import Checkbox from "../../atoms/Checkbox";
import type { AvatarBase, BodyId } from "../../data/randomIdentity";
import {
  randomName,
  randomAvatarBase,
  BODY_SHAPE_URNS,
  DEFAULT_WEARABLES,
} from "../../data/randomIdentity";
import type { WearableCatalogs } from "../../data/avatarRandomizer";
import {
  buildWearableCatalogs,
  selectRandomWearables,
} from "../../data/avatarRandomizer";
import { loadBackpack } from "../../data/catalyst/backpack";
import {
  getEngineAuthState,
  signOutEngineAuth,
  subscribeEngineAuth,
  type EngineAuthState,
} from "../../data/auth/engineLogin";
import { sendBridge } from "../../overlay/bridge";
import WearablePreview from "../../wearable-preview/WearablePreview";
import "./lobbynew.css";

const SignInFlow = lazy(() => import("../../overlay/SignInFlow"));

function shortAddress(address: string): string {
  return address.length > 12
    ? `${address.slice(0, 6)}\u{2026}${address.slice(-4)}`
    : address;
}

type JumpInPayload = { name: string; body: BodyId; base: AvatarBase; wearables: string[] };
type LobbyNewProps = { onJumpIn?: (payload: JumpInPayload) => void };

export default function LobbyNew({ onJumpIn }: LobbyNewProps = {}) {
  const [name, setName] = useState(randomName);
  const [body, setBody] = useState<BodyId>("A");
  const [agreed, setAgreed] = useState(false);
  const [tosNudge, setTosNudge] = useState(false);
  const [base, setBase] = useState<AvatarBase>(() => randomAvatarBase("", "A"));
  const [wearables, setWearables] = useState<string[]>(() => DEFAULT_WEARABLES.A);
  const [auth, setAuth] = useState<EngineAuthState>(getEngineAuthState);
  const [signInOpen, setSignInOpen] = useState(false);
  const [identityTouched, setIdentityTouched] = useState(false);
  const catalogsRef = useRef<WearableCatalogs | null>(null);

  useEffect(() => subscribeEngineAuth(setAuth), []);

  useEffect(() => {
    let cancelled = false;
    loadBackpack()
      .then((bp) => {
        if (!cancelled) catalogsRef.current = buildWearableCatalogs(bp?.catalog);
      })
      .catch(() => {
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // anything that marks the guest identity as touched also raises the terms, so the
  // nudge and the CTA emphasis can never disagree about whether they have committed
  function touchIdentity() {
    setIdentityTouched(true);
    if (!agreed) setTosNudge(true);
  }

  function applyAvatar(nextBase: AvatarBase, nextWears: string[]) {
    touchIdentity();
    setBase(nextBase);
    setWearables(nextWears);
    sendBridge("SetAvatar", {
      base: { ...nextBase, name: name.trim() || nextBase.name },
      equip: { wearableUrns: nextWears, emoteUrns: [], forceRender: [] },
    });
    sendBridge("RequestAvatarPreview", {});
  }

  function randomizeAvatar() {
    const nextBase = randomAvatarBase(name, body);
    const picked = catalogsRef.current
      ? selectRandomWearables(catalogsRef.current, body)
      : [];
    const nextWears = picked.length >= 4 ? picked : DEFAULT_WEARABLES[body];
    applyAvatar(nextBase, nextWears);
  }

  function chooseBody(x: BodyId) {
    if (x === body) return;
    setBody(x);
    applyAvatar(
      { ...base, bodyShapeUrn: BODY_SHAPE_URNS[x], name },
      DEFAULT_WEARABLES[x],
    );
  }

  const maleIcon = (
    <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true" fill="currentColor">
      <circle cx="12" cy="4" r="2.8" />
      <rect x="8.4" y="7.6" width="7.2" height="8.8" rx="1.6" />
      <rect x="9" y="15.6" width="2.3" height="5.4" rx="1" />
      <rect x="12.7" y="15.6" width="2.3" height="5.4" rx="1" />
    </svg>
  );
  const femaleIcon = (
    <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true" fill="currentColor">
      <circle cx="12" cy="4" r="2.8" />
      <path d="M12 7.4c2.1 0 3.2 1.5 3.8 3.5l1.6 5.6h-2.2l-.7 4.4H9.5l-.7-4.4H6.6l1.6-5.6C8.8 8.9 9.9 7.4 12 7.4z" />
    </svg>
  );

  // signing in is the headline offer until they invest in a guest identity; once they have
  // named the avatar or restyled it, jumping in with it is the thing they came to finish.
  const jumpLeads = Boolean(auth.address) || identityTouched;

  const avatar = (
    <>
      <div className="lobbynew__avatar">
        <WearablePreview
          outfit={{
            bodyShape: BODY_SHAPE_URNS[body],
            wearables,
            skin: { color: base.skinColor },
            hair: { color: base.hairColor },
            eyes: { color: base.eyesColor },
          }}
          emotes={["wave", "clap", "dab"]}
          spin
          controls={false}
          zoom={1.05}
        />
      </div>
      <div className="lobbynew__avatarbar">
        <div className="lobbynew__actions">
          <div
            className="lobbynew__bodyswitch"
            role="radiogroup"
            aria-label="Body type"
          >
            <button
              type="button"
              role="radio"
              aria-checked={body === "A"}
              aria-label="Masculine body"
              title="Masculine"
              className={"lobbynew__bodyswitch-btn" + (body === "A" ? " is-active" : "")}
              onClick={() => chooseBody("A")}
            >
              {maleIcon}
            </button>
            <button
              type="button"
              role="radio"
              aria-checked={body === "B"}
              aria-label="Feminine body"
              title="Feminine"
              className={"lobbynew__bodyswitch-btn" + (body === "B" ? " is-active" : "")}
              onClick={() => chooseBody("B")}
            >
              {femaleIcon}
            </button>
          </div>
          <button
            type="button"
            className="lobbynew__randomize"
            onClick={randomizeAvatar}
            title="Randomize avatar"
          >
            <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true">
              <rect x="3" y="3" width="18" height="18" rx="4" fill="none" stroke="currentColor" strokeWidth="2" />
              <circle cx="8" cy="8" r="1.5" fill="currentColor" /><circle cx="16" cy="8" r="1.5" fill="currentColor" />
              <circle cx="12" cy="12" r="1.5" fill="currentColor" />
              <circle cx="8" cy="16" r="1.5" fill="currentColor" /><circle cx="16" cy="16" r="1.5" fill="currentColor" />
            </svg>
            <span>Random</span>
          </button>
        </div>

        <div className="lobbynew__caption">
          You can customize your avatar later.
        </div>
      </div>
    </>
  );

  return (
    <AuthLayout
      avatar={avatar}
      hideBrand
      hideFooter
    >
      <div className="lobbynew__head">
        <span className="lobbynew__logo" aria-hidden="true">
          <svg viewBox="0 0 40 40" width="40" height="40">
            <defs>
              <linearGradient
                id="lobbynew-logo-g"
                x1="6"
                y1="6"
                x2="34"
                y2="34"
                gradientUnits="userSpaceOnUse"
              >
                <stop offset="0" stopColor="#ff7d68" />
                <stop offset="1" stopColor="#e91e9e" />
              </linearGradient>
              <clipPath id="lobbynew-logo-c">
                <circle cx="20" cy="20" r="20" />
              </clipPath>
            </defs>
            <circle cx="20" cy="20" r="20" fill="url(#lobbynew-logo-g)" />
            <g clipPath="url(#lobbynew-logo-c)">
              <circle cx="14.5" cy="11" r="2.4" fill="#ff9d7a" />
              <circle cx="25" cy="14.5" r="4.6" fill="#ff9d7a" />
              <path d="M3 30 15 14l11 16z" fill="#f3eefb" />
              <path d="M18 30 26.5 19 36 30z" fill="#f3eefb" />
              <rect x="0" y="29" width="40" height="11" fill="#ff8d6e" />
            </g>
          </svg>
        </span>
        <h1 className="lobbynew__title">Welcome to Decentraland!</h1>
      </div>

      <div className="lobbynew__group">
        <label className="lobbynew__label" htmlFor="lobbynew-name">
          Username
        </label>
        <div className="lobbynew__namerow">
          <input
            id="lobbynew-name"
            className="lobbynew__field"
            aria-label="Username"
            placeholder="Name"
            maxLength={15}
            value={name}
            onChange={(e) => {
              touchIdentity();
              setName(e.target.value);
            }}
          />
          <button
            type="button"
            className="lobbynew__namernd"
            onClick={() => {
              touchIdentity();
              setName(randomName());
            }}
            title="Random name"
            aria-label="Random name"
          >
            <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
              <rect x="3" y="3" width="18" height="18" rx="4" fill="none" stroke="currentColor" strokeWidth="2" />
              <circle cx="8" cy="8" r="1.5" fill="currentColor" /><circle cx="16" cy="8" r="1.5" fill="currentColor" />
              <circle cx="12" cy="12" r="1.5" fill="currentColor" />
              <circle cx="8" cy="16" r="1.5" fill="currentColor" /><circle cx="16" cy="16" r="1.5" fill="currentColor" />
            </svg>
          </button>
        </div>
        <div className="lobbynew__count">{name.length}/15</div>
      </div>

      <div className={"lobbynew__checks" + (tosNudge ? " is-nudged" : "")}>
        <Checkbox
          checked={agreed}
          onChange={(on) => {
            setAgreed(on);
            if (on) setTosNudge(false);
          }}
        >
          I agree with Decentraland&#x2019;s{" "}
          <a
            className="lobbynew__toslink"
            href={siteUrl("/terms")}
            target="_blank"
            rel="noopener noreferrer"
          >
            Terms of Use
          </a>{" "}
          and{" "}
          <a
            className="lobbynew__toslink"
            href={siteUrl("/privacy")}
            target="_blank"
            rel="noopener noreferrer"
          >
            Privacy Policy
          </a>{" "}
          *
        </Checkbox>
        {tosNudge && !agreed ? (
          <div className="lobbynew__tosnudge" role="alert">
            Accept the terms above to jump in.
          </div>
        ) : null}
      </div>

      <div className="lobbynew__cta">
        <div
          className="lobbynew__jumpwrap"
          onClick={() => {
            if (!agreed) setTosNudge(true);
          }}
        >
          <button
            className={
              "lobbynew__btn lobbynew__jump " +
              (jumpLeads ? "is-primary" : "is-secondary")
            }
            data-sb-linkto="Explorer/Workflows/Loading"
            disabled={!agreed}
            onClick={() => onJumpIn?.({ name: name.trim(), body, base, wearables })}
          >
            <span className="lobbynew__jump-label">Continue as guest</span>
            <span className="lobbynew__jump-icon" aria-hidden="true">
              <svg viewBox="0 0 24 24" width="16" height="16">
                <path
                  d="M5 12h12m0 0-4-4m4 4-4 4"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2.2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
            </span>
          </button>
        </div>

        {auth.address ? null : (
          <button
            type="button"
            className={
              "lobbynew__btn lobbynew__signin " +
              (jumpLeads ? "is-secondary" : "is-primary")
            }
            onClick={() => setSignInOpen(true)}
          >
            Sign in
          </button>
        )}
      </div>

      {auth.address ? (
        <div className="lobbynew__auth">
          <span className="lobbynew__auth-state">
            {auth.status === "signedIn" ? "Signed in as " : "Signing in as "}
            <b title={auth.address}>{shortAddress(auth.address)}</b>
          </span>
          <button
            type="button"
            className="lobbynew__auth-link"
            onClick={() => signOutEngineAuth()}
          >
            Sign out
          </button>
        </div>
      ) : null}

      {signInOpen && (
        <Suspense fallback={null}>
          <SignInFlow
            onClose={() => setSignInOpen(false)}
            onSignedIn={() => setSignInOpen(false)}
          />
        </Suspense>
      )}
    </AuthLayout>
  );
}
