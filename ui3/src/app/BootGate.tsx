import type { ReactNode } from "react";
import { useEffect, useRef, useState } from "react";

import LobbyNew from "../explorer/workflows/LobbyNew";
import Loading from "../explorer/workflows/Loading";
import FpsMeter from "../explorer/components/FpsMeter";
import type { OverlayPush } from "../generated/bridge/OverlayPush";
import { hexToColor3 } from "../data/catalyst/backpack";
import {
  IDENTITY_STORAGE_KEY,
  initEngineAuth,
  shouldAutoJumpIn,
  getEngineAuthState,
  subscribeEngineAuth,
} from "../data/auth/engineLogin";
import { randomName } from "../data/randomIdentity";
import { subscribeLifecycle } from "../overlay/bridge";
import { WorldEntryContext, type EntryDestination } from "./WorldEntry";
import "./bootgate.css";

function bootWin(): Window | null {
  return typeof window !== "undefined" ? window : null;
}

if (typeof window !== "undefined") {
  window.dclDeferStart = true;
}

const ANTI_STRAND_MS = 75000;
const LOADING_TIMEOUT_MS = 20000;
const MAX_NAME_REASSERTS = 3;

const ONBOARD_BODY_SHAPE = {
  A: "urn:decentraland:off-chain:base-avatars:BaseMale",
  B: "urn:decentraland:off-chain:base-avatars:BaseFemale",
} as const;
const ONBOARD_DEFAULT_COLORS = {
  skinColor: "#c98c63",
  hairColor: "#5c3824",
  eyesColor: "#3a6ea5",
};

type PendingAvatar = {
  name?: string;
  fallbackName?: string;
  bodyShapeUrn: string;
  skinColor: unknown;
  hairColor: unknown;
  eyesColor: unknown;
  wearables: string[] | null;
};

type JumpInArg = {
  name?: string;
  body?: "A" | "B";
  base?: {
    bodyShapeUrn?: string;
    name?: string;
    skinColor?: unknown;
    hairColor?: unknown;
    eyesColor?: unknown;
  };
  wearables?: string[];
};

export function buildJumpInAvatarPayload(pending: PendingAvatar): {
  base: Record<string, unknown>;
  equip?: Record<string, unknown>;
} {
  const base: Record<string, unknown> = {
    bodyShapeUrn: pending.bodyShapeUrn,
    skinColor: pending.skinColor,
    hairColor: pending.hairColor,
    eyesColor: pending.eyesColor,
    name: pending.name || pending.fallbackName || randomName(),
  };
  const payload: { base: Record<string, unknown>; equip?: Record<string, unknown> } =
    { base };
  if (Array.isArray(pending.wearables) && pending.wearables.length) {
    payload.equip = {
      wearableUrns: pending.wearables,
      emoteUrns: [],
      forceRender: [],
    };
  }
  return payload;
}

export function destinationFromSearch(search: string): EntryDestination {
  let params: URLSearchParams;
  try {
    params = new URLSearchParams(search);
  } catch {
    return null;
  }
  const realm = params.get("realm")?.trim();
  const position = params.get("position")?.trim();
  const coords = position ? /^(-?\d{1,4}),(-?\d{1,4})$/.exec(position) : null;
  if (realm) return {
    kind: "world",
    realm: realm.startsWith("/") && !realm.startsWith("//") ? window.location.origin + realm : realm,
    ...(coords ? { parcel: [Number(coords[1]), Number(coords[2])] as [number, number] } : {}),
  };
  if (coords) return { kind: "parcel", x: Number(coords[1]), y: Number(coords[2]) };
  return null;
}

type BootGateProps = { children: ReactNode };

export default function BootGate({ children }: BootGateProps) {
  const [showFps] = useState(() => {
    try {
      return new URLSearchParams(window.location.search).get("fps") === "1";
    } catch {
      return false;
    }
  });
  return (
    <>
      <BootPhases>{children}</BootPhases>
      {showFps ? <FpsMeter /> : null}
    </>
  );
}

function BootPhases({ children }: BootGateProps) {
  const [auth, setAuth] = useState(getEngineAuthState);
  useEffect(() => subscribeEngineAuth(setAuth), []);
  const [hasIdentity] = useState(() => {
    let raw: string | null = null;
    try {
      raw = localStorage.getItem(IDENTITY_STORAGE_KEY);
    } catch {
      return false;
    }
    return shouldAutoJumpIn(raw);
  });
  const [deepLink] = useState<EntryDestination>(() => {
    const w = bootWin();
    return w ? destinationFromSearch(w.location.search) : null;
  });
  const autoEnter = useRef(!!deepLink && (deepLink.kind === "parcel" || !!deepLink.parcel));
  const [phase, setPhase] = useState<"onboarding" | "lobby" | "loading" | "world" | "stalled">(
    hasIdentity ? (autoEnter.current ? "loading" : "lobby") : "onboarding",
  );
  useEffect(() => {
    document.documentElement.dataset.dclBootPhase = phase;
  }, [phase]);
  const [wasmPct, setWasmPct] = useState(() => {
    const w = bootWin();
    return typeof w?.dclLoadingProgress === "number" ? w.dclLoadingProgress : 0;
  });
  const [scenePct, setScenePct] = useState(0);
  const [ready, setReady] = useState(false);
  const [degraded, setDegraded] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [startupTimedOut, setStartupTimedOut] = useState(false);
  const [pendingAssets, setPendingAssets] = useState(0);
  const [avatarReady, setAvatarReady] = useState(false);
  const [engineAlive, setEngineAlive] = useState(false);
  const [destinationSettled, setDestinationSettled] = useState(true);
  const lifecycleSeenRef = useRef(false);
  const startGeneration = useRef(0);
  const startListener = useRef<(() => void) | null>(null);
  const engineAliveAt = useRef(0);
  const jumpedAt = useRef(0);
  const pendingAvatarRef = useRef<PendingAvatar | null>(null);
  const avatarAppliedRef = useRef(false);
  const guestRequestedRef = useRef(false);
  const appliedAvatarRef = useRef<ReturnType<typeof buildJumpInAvatarPayload> | null>(
    null,
  );
  const nameReassertsRef = useRef(0);
  const avatarSignalSeenRef = useRef(false);
  const initialDestinationRef = useRef<EntryDestination>(null);

  const ensureGuestIdentity = () => {
    if (!pendingAvatarRef.current || guestRequestedRef.current || getEngineAuthState().address) return;
    const bridge = bootWin()?.dclBridge;
    if (!bridge) return;
    guestRequestedRef.current = true;
    bridge.send("LoginGuest", {});
  };

  const applyPendingAvatar = () => {
    const pending = pendingAvatarRef.current;
    if (!pending || avatarAppliedRef.current) return;
    avatarAppliedRef.current = true;
    const payload = buildJumpInAvatarPayload(pending);
    appliedAvatarRef.current = payload;
    try {
      bootWin()?.dclBridge?.send?.("SetAvatar", payload);
    } catch {
    }
  };

  const reassertChosenName = (reported: string) => {
    const payload = appliedAvatarRef.current;
    const chosen = payload?.base.name;
    if (!payload || typeof chosen !== "string" || reported === chosen) return;
    if (nameReassertsRef.current >= MAX_NAME_REASSERTS) return;
    nameReassertsRef.current += 1;
    try {
      bootWin()?.dclBridge?.send?.("SetAvatar", payload);
    } catch {
    }
  };

  const startEngine = (lobby: boolean) => {
    const bw = bootWin();
    if (startListener.current) window.removeEventListener("dcl-engine-ready", startListener.current);
    const generation = ++startGeneration.current;
    const fail = (error: unknown) => {
      if (generation === startGeneration.current) {
        setStartupTimedOut(false);
        setFailure(error instanceof Error ? error.message : "Engine startup failed.");
        setPhase("stalled");
      }
    };
    const apply = () => {
      if (generation !== startGeneration.current) return;
      if (!lobby) setDestinationSettled(true);
    };
    const start = () => {
      if (generation !== startGeneration.current) return;
      try {
        const dest = initialDestinationRef.current;
        const destination = dest?.kind === "world" ? { realm: dest.realm, parcel: dest.parcel }
          : dest?.kind === "parcel" ? { realm: "", parcel: [dest.x, dest.y] as [number, number] } : undefined;
        const running = lobby ? bootWin()?.dclEngineConnectLobby?.() : bootWin()?.dclEngineStart?.(destination);
        if (running) void running.then(apply, fail);
        else apply();
      } catch (error) { fail(error); }
    };
    startListener.current = start;
    if (bw?.dclEngineReady) start();
    else
      window.addEventListener(
        "dcl-engine-ready",
        start,
        { once: true },
      );
  };

  useEffect(() => () => {
    startGeneration.current++;
    if (startListener.current) window.removeEventListener("dcl-engine-ready", startListener.current);
  }, []);

  useEffect(() => subscribeLifecycle((snapshot) => {
    lifecycleSeenRef.current = true;
    if (engineAliveAt.current === 0) engineAliveAt.current = Date.now();
    setEngineAlive(true);
    ensureGuestIdentity();
    setReady(snapshot.readiness.canExplore);
    setDegraded(snapshot.readiness.placement === "degraded" || snapshot.travel?.phase === "degraded");
    setPendingAssets(snapshot.readiness.pendingAssets);
    avatarSignalSeenRef.current = true;
    setAvatarReady(snapshot.readiness.avatarReady);
    if (snapshot.realm.phase === "failed" || snapshot.travel?.phase === "failed") {
      setStartupTimedOut(false);
      setFailure(snapshot.realm.error ?? snapshot.travel?.blockingReason ?? "The destination could not load.");
      setPhase((current) => current === "loading" ? "stalled" : current);
    }
  }), []);

  useEffect(() => {
    const onLoading = (e: Event) => {
      const p = (e as CustomEvent<{ percent?: number }>).detail?.percent;
      if (typeof p === "number") setWasmPct(p);
    };
    window.addEventListener("dcl-loading", onLoading);
    return () => window.removeEventListener("dcl-loading", onLoading);
  }, []);

  useEffect(() => {
    initEngineAuth();
    if (window.location.search.includes("authResult=")) {
      void import("../data/auth/socialRedirect").then((m) =>
        m.completeSocialRedirectLogin(),
      );
    }
  }, []);

  useEffect(() => {
    if (phase === "loading" && autoEnter.current) {
      autoEnter.current = false;
      handleDestinationChosen(deepLink);
    } else if (phase === "lobby") startEngine(true);
  }, [phase]);

  useEffect(() => {
    let unsub: (() => void) | undefined;
    let cancelled = false;
    const attach = () => {
      if (cancelled) return;
      const b = bootWin()?.dclBridge;
      if (b && typeof b.onState === "function") {
        unsub = b.onState((raw) => {
          const push = raw as OverlayPush | null;
          if (!push) return;
          if (push.kind === "loading") {
            if (engineAliveAt.current === 0) engineAliveAt.current = Date.now();
            setEngineAlive(true);
            ensureGuestIdentity();
            if (typeof push.percent === "number") setScenePct(push.percent);
            if (!lifecycleSeenRef.current) setReady(push.ready);
            if (!lifecycleSeenRef.current && typeof push.pendingAssets === "number") setPendingAssets(push.pendingAssets);
            if (!lifecycleSeenRef.current && typeof push.avatarLoaded === "boolean") {
              avatarSignalSeenRef.current = true;
              if (push.avatarLoaded) setAvatarReady(true);
            }
          } else if (push.kind === "identity") {
            ensureGuestIdentity();
            if (push.name) {
              if (avatarAppliedRef.current) {
                reassertChosenName(push.name);
              } else {
                applyPendingAvatar();
              }
            }
          }
        });
        return;
      }
      setTimeout(attach, 250);
    };
    attach();
    return () => {
      cancelled = true;
      if (unsub) unsub();
    };
  }, []);

  useEffect(() => {
    if (phase !== "loading" && !(phase === "stalled" && startupTimedOut)) return undefined;
    const avatarGateSatisfied = !avatarSignalSeenRef.current || avatarReady;
    if (destinationSettled && ready && (degraded || (pendingAssets === 0 && avatarGateSatisfied))) {
      setStartupTimedOut(false);
      setFailure(null);
      setPhase("world");
      return;
    }
    if (phase !== "loading") return undefined;
    const deadline =
      engineAliveAt.current > 0
        ? jumpedAt.current + ANTI_STRAND_MS
        : jumpedAt.current + LOADING_TIMEOUT_MS;
    const fallback = setTimeout(
      () => {
        setStartupTimedOut(true);
        setFailure("The explorer did not report readiness in time.");
        setPhase("stalled");
      },
      Math.max(0, deadline - Date.now()),
    );
    return () => {
      clearTimeout(fallback);
    };
  }, [phase, ready, degraded, engineAlive, avatarReady, pendingAssets, destinationSettled, startupTimedOut]);

  const handleAvatarChosen = ({ name, body, base, wearables }: JumpInArg = {}) => {
    const trimmed = (name ?? "").trim();
    pendingAvatarRef.current = {
      name: trimmed,
      fallbackName: base?.name,
      bodyShapeUrn:
        base?.bodyShapeUrn ?? (body ? ONBOARD_BODY_SHAPE[body] : undefined) ?? ONBOARD_BODY_SHAPE.A,
      skinColor: base?.skinColor ?? hexToColor3(ONBOARD_DEFAULT_COLORS.skinColor),
      hairColor: base?.hairColor ?? hexToColor3(ONBOARD_DEFAULT_COLORS.hairColor),
      eyesColor: base?.eyesColor ?? hexToColor3(ONBOARD_DEFAULT_COLORS.eyesColor),
      wearables: Array.isArray(wearables) ? wearables : null,
    };
    avatarAppliedRef.current = false;
    if (engineAliveAt.current > 0) ensureGuestIdentity();
    if (autoEnter.current) {
      autoEnter.current = false;
      handleDestinationChosen(deepLink);
    } else setPhase("lobby");
  };

  const handleDestinationChosen = (dest: EntryDestination) => {
    initialDestinationRef.current = dest ?? deepLink;
    setDestinationSettled(false);
    setStartupTimedOut(false);
    setFailure(null);
    jumpedAt.current = Date.now();
    setPhase("loading");
    startEngine(false);
  };

  useEffect(() => {
    if (phase !== "onboarding" || !auth.address) return;
    pendingAvatarRef.current = null;
    if (autoEnter.current) {
      autoEnter.current = false;
      handleDestinationChosen(deepLink);
    } else setPhase("lobby");
  }, [phase, auth.address]);

  if (phase === "onboarding") {
    return (
      <div className="boot">
        <LobbyNew onJumpIn={handleAvatarChosen} />
      </div>
    );
  }
  if (phase === "loading") {
    const pct =
      ready && pendingAssets === 0
        ? 100
        : Math.min(99, Math.round(scenePct > 0 ? 50 + scenePct * 0.5 : wasmPct * 0.5));
    return (
      <div className="boot">
        <Loading progress={pct} />
      </div>
    );
  }
  if (phase === "stalled") {
    return (
      <div className="boot">
        <div className="boot__stalled" role="alert">
          <h1 className="boot__stalled-title">The world couldn&#x2019;t start</h1>
          <p className="boot__stalled-body">
            {failure ?? "The explorer could not finish starting. Try again or choose another destination."}
          </p>
          <div className="boot__stalled-actions">
            <button
              type="button"
              className="boot__stalled-btn boot__stalled-btn--primary"
              onClick={() => window.location.reload()}
            >
              Try again
            </button>
            <button
              type="button"
              className="boot__stalled-btn"
              onClick={() => {
                startGeneration.current++;
                setPhase("lobby");
              }}
            >
              Back to lobby
            </button>
          </div>
        </div>
      </div>
    );
  }
  return <WorldEntryContext.Provider value={{ pending: phase === "lobby", destination: deepLink, enter: handleDestinationChosen }}>
    {children}
  </WorldEntryContext.Provider>;
}
