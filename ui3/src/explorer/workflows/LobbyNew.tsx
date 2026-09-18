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
import { useQueryClient } from "@tanstack/react-query";
import { readWearables } from "../../data/hooks/useOwnedItems";
import {
  getEngineAuthState,
  signOutEngineAuth,
  subscribeEngineAuth,
  type EngineAuthState,
} from "../../data/auth/engineLogin";
import WearablePreview from "../../wearable-preview/WearablePreview";
import heroArt from "./lobby-new/AuthScreenImg.webp";
import heroArtSmall from "./lobby-new/AuthScreenImg-860.webp";
import "./lobbynew.css";

const SignInFlow = lazy(() => import("../../overlay/SignInFlow"));

function shortAddress(address: string): string {
  return address.length > 12
    ? `${address.slice(0, 6)}\u{2026}${address.slice(-4)}`
    : address;
}

type JumpInPayload = { name: string; body: BodyId; base: AvatarBase; wearables: string[] };
type LobbyNewProps = { onJumpIn?: (payload: JumpInPayload) => void };
type LobbyStep = "welcome" | "identity";

export default function LobbyNew({ onJumpIn }: LobbyNewProps = {}) {
  const [step, setStep] = useState<LobbyStep>("welcome");
  const [name, setName] = useState(randomName);
  const [body, setBody] = useState<BodyId>("A");
  const [agreed, setAgreed] = useState(false);
  const [tosNudge, setTosNudge] = useState(false);
  const [base, setBase] = useState<AvatarBase>(() => randomAvatarBase("", "A"));
  const [wearables, setWearables] = useState<string[]>(() => DEFAULT_WEARABLES.A);
  const [auth, setAuth] = useState<EngineAuthState>(getEngineAuthState);
  const [signInOpen, setSignInOpen] = useState(false);
  const catalogsRef = useRef<WearableCatalogs | null>(null);
  const queryClient = useQueryClient();

  useEffect(() => subscribeEngineAuth(setAuth), []);

  useEffect(() => {
    if (auth.address) setStep("welcome");
  }, [auth.address]);

  useEffect(() => {
    let cancelled = false;
    readWearables(queryClient)
      .then((bp) => {
        if (!cancelled) catalogsRef.current = buildWearableCatalogs(bp?.catalog);
      })
      .catch(() => {
      });
    return () => {
      cancelled = true;
    };
  }, [queryClient]);

  function touchIdentity() {
    if (!agreed) setTosNudge(true);
  }

  function applyAvatar(nextBase: AvatarBase, nextWears: string[]) {
    touchIdentity();
    setBase(nextBase);
    setWearables(nextWears);
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
  const diceIcon = (size: number) => (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
      <rect x="3" y="3" width="18" height="18" rx="4" fill="none" stroke="currentColor" strokeWidth="2" />
      <circle cx="8" cy="8" r="1.5" fill="currentColor" /><circle cx="16" cy="8" r="1.5" fill="currentColor" />
      <circle cx="12" cy="12" r="1.5" fill="currentColor" />
      <circle cx="8" cy="16" r="1.5" fill="currentColor" /><circle cx="16" cy="16" r="1.5" fill="currentColor" />
    </svg>
  );

  const identity = step === "identity";

  const stage = (
    <>
      <div className="lobbynew__backdrop" aria-hidden="true">
        <div className="lobbynew__pattern">
          <div className="lobbynew__pattern-x">
            <div className="lobbynew__pattern-y" />
          </div>
        </div>
        <div className="lobbynew__vignette" />
      </div>
      <img
        className="lobbynew__art"
        src={heroArtSmall}
        srcSet={`${heroArtSmall} 860w, ${heroArt} 1720w`}
        sizes="(max-width: 820px) 90vw, 80vh"
        alt=""
        decoding="async"
        draggable={false}
      />
      <div className="lobbynew__avatar" aria-hidden={!identity}>
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
          pitch={18}
        />
      </div>
      {identity ? (
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
              {diceIcon(15)}
              <span>Random</span>
            </button>
          </div>

          <div className="lobbynew__caption">
            You can customize your avatar later.
          </div>
        </div>
      ) : null}
    </>
  );

  const logo = (
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
  );

  const welcome = (
    <>
      <div className="lobbynew__head">
        {logo}
        <h1 className="lobbynew__title">Welcome to Decentraland!</h1>
        <p className="lobbynew__subtitle">Discover a world shaped by its community!</p>
      </div>

      <div className="lobbynew__cta">
        <button
          type="button"
          className="lobbynew__btn lobbynew__guest is-primary"
          disabled={!!auth.address}
          onClick={() => setStep("identity")}
        >
          {auth.address ? "Entering Decentraland\u{2026}" : "Play as a guest"}
        </button>

        {auth.address ? null : (
          <button
            type="button"
            className="lobbynew__btn lobbynew__signin is-secondary"
            onClick={() => setSignInOpen(true)}
          >
            Login or sign up
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
    </>
  );

  const pickName = (
    <>
      <div className="lobbynew__head">
        <h1 className="lobbynew__title">Pick Your Name</h1>
      </div>

      <div className="lobbynew__group">
        <label className="lobbynew__label" htmlFor="lobbynew-name">
          Username*
        </label>
        <div className="lobbynew__namerow">
          <input
            id="lobbynew-name"
            className="lobbynew__field"
            aria-label="Username"
            placeholder="Enter your username"
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
            {diceIcon(16)}
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
            type="button"
            className="lobbynew__btn lobbynew__jump is-primary"
            data-sb-linkto="Explorer/Workflows/Loading"
            disabled={!agreed}
            onClick={() => onJumpIn?.({ name: name.trim(), body, base, wearables })}
          >
            <span className="lobbynew__jump-label">Let&#x2019;s go</span>
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
      </div>
    </>
  );

  const back = identity ? (
    <button
      type="button"
      className="lobbynew__back"
      onClick={() => {
        setTosNudge(false);
        setStep("welcome");
      }}
    >
      <span className="lobbynew__back-icon" aria-hidden="true">
        <svg viewBox="0 0 24 24" width="16" height="16">
          <path
            d="M14.5 6 8.5 12l6 6"
            fill="none"
            stroke="currentColor"
            strokeWidth="2.4"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </span>
      <span>Back</span>
    </button>
  ) : undefined;

  return (
    <AuthLayout
      avatar={stage}
      topLeft={back}
      hideBrand
      hideFooter
    >
      <div className={"lobbynew lobbynew--" + step}>
        {identity ? pickName : welcome}
      </div>

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
