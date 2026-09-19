import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import Button from "../../atoms/Button";
import Modal from "../../components/Modal";
import { connectWallet, walletProvider } from "../../data/auth/wallet";
import {
  REQUEST_CONFIRM_TIMEOUT_MS,
  feedbackRoute,
  sendOwnerMessage,
  type FeedbackRoute,
} from "../../data/catalyst/sceneFeedback";
import {
  SCENE_OWNER_STALE_MS,
  fetchHomeRealm,
  fetchSceneDeployment,
  fetchWorldOwner,
  worldRealmBase,
  isHomeRealm,
  sceneOwnerKeys,
} from "../../data/catalyst/sceneOwner";
import { truncateAddress } from "../../data/format";
import { FRIEND_ACTIONS, requestFriendAction, useFriends } from "../../data/hooks/useFriends";
import { useBridgeState } from "../../overlay/bridge";
import { chainName, fetchTipTarget, manaToWei, sendManaTip, type TipTarget } from "./manaTip";
import "./sceneowneractions.css";

const COORDS_RE = /^\s*(-?\d+)\s*,\s*(-?\d+)\s*$/;

export function normalizeParcel(coords: string | null | undefined): string | null {
  if (typeof coords !== "string") return null;
  const m = coords.match(COORDS_RE);
  return m ? `${Number(m[1])},${Number(m[2])}` : null;
}

export type SceneOwner = {
  address: string | null;
  sceneTitle: string | null;
  loading: boolean;
  world: boolean;
};

export function useSceneOwner(
  coords: string | null | undefined,
  realm: string | null | undefined,
  enabled = true,
): SceneOwner {
  const parcel = normalizeParcel(coords);
  const realmKnown = typeof realm === "string" && realm.trim() !== "";
  const wanted = enabled && parcel != null && realmKnown;
  const home = useQuery({
    queryKey: sceneOwnerKeys.homeRealm(),
    queryFn: ({ signal }) => fetchHomeRealm({ signal }),
    staleTime: Infinity,
    enabled: wanted,
  });
  const land = home.data != null && isHomeRealm(realm, home.data);
  const worldBase = realm ? worldRealmBase(realm, typeof window === "undefined" ? null : new URLSearchParams(window.location.search).get("realm")) : null;
  const worldOwner = useQuery({
    queryKey: sceneOwnerKeys.world(realm, worldBase),
    queryFn: ({ signal }) => fetchWorldOwner(realm ?? "", worldBase ?? "", { signal }),
    staleTime: SCENE_OWNER_STALE_MS,
    enabled: wanted && home.data != null && !land && worldBase != null,
  });
  const deployment = useQuery({
    queryKey: sceneOwnerKeys.deployment(parcel),
    queryFn: ({ signal }) => fetchSceneDeployment(parcel ?? "", { signal }),
    staleTime: SCENE_OWNER_STALE_MS,
    enabled: wanted && land,
  });
  return {
    address: (land ? deployment.data?.deployer : worldOwner.data?.deployer) ?? null,
    sceneTitle: (land ? deployment.data?.title : worldOwner.data?.title) ?? null,
    loading: wanted && (home.isFetching || (land ? deployment.isFetching : worldOwner.isFetching)),
    world: realmKnown && home.data != null && !land,
  };
}

export const FEEDBACK_MAX = 1000;

export function feedbackHeader(sceneTitle: string, coords: string): string {
  return `Feedback on ${sceneTitle.trim() || "your scene"} (${coords}): `;
}

export function composeFeedback(sceneTitle: string, coords: string, text: string): string {
  return (feedbackHeader(sceneTitle, coords) + text.trim()).slice(0, FEEDBACK_MAX);
}

type OwnerModalProps = {
  owner: string;
  sceneTitle: string;
  coords: string;
  onClose: () => void;
};

function OwnerMeta({ sceneTitle, coords, owner }: Omit<OwnerModalProps, "onClose">) {
  return (
    <p className="soa__meta">
      <span className="soa__scene">{sceneTitle || "This scene"}</span>
      <span aria-hidden="true"> &middot; </span>
      <span>{coords}</span>
      <span aria-hidden="true"> &middot; </span>
      <span>
        owner <span className="u-wallet">{truncateAddress(owner)}</span>
      </span>
    </p>
  );
}

type FeedbackPhase =
  | { kind: "compose" }
  | { kind: "sending" }
  | { kind: "confirming" }
  | { kind: "messaged" }
  | { kind: "requested" }
  | { kind: "unconfirmed" }
  | { kind: "error"; message: string };

const ROUTE_HINT: Record<FeedbackRoute, string> = {
  message: "delivered as a direct message, you are friends",
  request: "delivered as a friend request with your note attached",
  pending: "",
  incoming: "",
};

function FeedbackOutcome({ phase, onClose }: { phase: FeedbackPhase; onClose: () => void }) {
  const text =
    phase.kind === "messaged"
      ? "Delivered. Your note reached the owner as a direct message."
      : phase.kind === "requested"
        ? "Your friend request is on its way with the note attached; the owner will find it under Friends \u203a Requests."
        : "The client accepted the request but did not confirm it went out. Check Friends \u203a Requests before sending it again.";
  return (
    <>
      <p className={phase.kind === "unconfirmed" ? "soa__warn" : "soa__ok"}>{text}</p>
      <div className="soa__actions">
        <Button onClick={onClose}>Done</Button>
      </div>
    </>
  );
}

export function SceneFeedbackModal({ owner, sceneTitle, coords, onClose }: OwnerModalProps) {
  const identity = useBridgeState((s) => s.identity);
  const { friends, sent, received } = useFriends();
  const route = feedbackRoute(owner, friends, sent, received);
  const [text, setText] = useState("");
  const [phase, setPhase] = useState<FeedbackPhase>({ kind: "compose" });
  const self = (identity.address ?? "").toLowerCase() === owner.toLowerCase();
  const remaining = FEEDBACK_MAX - feedbackHeader(sceneTitle, coords).length;
  const body = text.trim();
  const routeOpen = route === "message" || route === "request";
  const composing = phase.kind === "compose" || phase.kind === "error";
  const canSend =
    composing && !identity.isGuest && !self && routeOpen && body.length > 0 && body.length <= remaining;
  const requestSeen = route === "pending" || route === "message";

  useEffect(() => {
    if (phase.kind !== "confirming") return undefined;
    if (requestSeen) {
      setPhase({ kind: "requested" });
      return undefined;
    }
    const timer = setTimeout(() => setPhase({ kind: "unconfirmed" }), REQUEST_CONFIRM_TIMEOUT_MS);
    return () => clearTimeout(timer);
  }, [phase.kind, requestSeen]);

  async function send() {
    if (!canSend) return;
    const note = composeFeedback(sceneTitle, coords, text);
    if (route === "message") {
      setPhase({ kind: "sending" });
      try {
        await sendOwnerMessage(owner, note);
        setPhase({ kind: "messaged" });
      } catch (e) {
        setPhase({
          kind: "error",
          message: e instanceof Error && e.message ? e.message : "The message was not delivered.",
        });
      }
      return;
    }
    const handed = requestFriendAction(FRIEND_ACTIONS.REQUEST, owner, { message: note });
    setPhase(
      handed
        ? { kind: "confirming" }
        : { kind: "error", message: "The friend request could not be handed to the client." },
    );
  }

  const done = phase.kind === "messaged" || phase.kind === "requested" || phase.kind === "unconfirmed";
  const busy = phase.kind === "sending" || phase.kind === "confirming";

  return (
    <Modal onClose={onClose} ariaLabel="Send feedback to scene owner" className="soa" width={440}>
      <h2 className="soa__title">Send feedback to the scene owner</h2>
      <OwnerMeta sceneTitle={sceneTitle} coords={coords} owner={owner} />
      {done ? (
        <FeedbackOutcome phase={phase} onClose={onClose} />
      ) : (
        <>
          <textarea
            className="soa__text"
            value={text}
            onChange={(e) => setText(e.target.value)}
            maxLength={remaining}
            rows={5}
            placeholder="What worked, what broke, what you would love to see next&hellip;"
            aria-label="Feedback"
            disabled={busy}
            autoFocus
          />
          <div className="soa__hint">
            {body.length}/{remaining}
            {ROUTE_HINT[route] ? ` \u00b7 ${ROUTE_HINT[route]}` : ""}
          </div>
          {identity.isGuest && <p className="soa__warn">Sign in with a wallet to send feedback.</p>}
          {!identity.isGuest && self && <p className="soa__warn">You own this scene.</p>}
          {route === "pending" && (
            <p className="soa__warn">
              Your friend request to the owner is still pending. The note can go out as a message
              once they accept it.
            </p>
          )}
          {route === "incoming" && (
            <p className="soa__warn">
              The owner already sent you a friend request. Accept it under Friends &rsaquo; Requests,
              then send your feedback as a message.
            </p>
          )}
          {phase.kind === "error" && <p className="soa__warn soa__err">{phase.message}</p>}
          <div className="soa__actions">
            <Button variant="secondary" onClick={onClose}>
              Cancel
            </Button>
            <Button onClick={send} disabled={!canSend}>
              {phase.kind === "sending"
                ? "Sending\u2026"
                : phase.kind === "confirming"
                  ? "Confirming\u2026"
                  : route === "message"
                    ? "Send message"
                    : "Send feedback"}
            </Button>
          </div>
        </>
      )}
    </Modal>
  );
}

const QUICK_AMOUNTS = ["1", "5", "10", "50"];

type TipPhase =
  | { kind: "loading" }
  | { kind: "unavailable"; reason: string }
  | { kind: "idle" }
  | { kind: "sending" }
  | { kind: "sent"; hash: string }
  | { kind: "error"; message: string };

function copyToClipboard(text: string) {
  try {
    navigator.clipboard?.writeText(text);
  } catch {
  }
}

export function SceneTipModal({ owner, sceneTitle, coords, onClose }: OwnerModalProps) {
  const identity = useBridgeState((s) => s.identity);
  const [amount, setAmount] = useState("5");
  const [target, setTarget] = useState<TipTarget | null>(null);
  const [phase, setPhase] = useState<TipPhase>({ kind: "loading" });
  const [copied, setCopied] = useState(false);
  const wei = useMemo(() => manaToWei(amount), [amount]);
  const self = (identity.address ?? "").toLowerCase() === owner.toLowerCase();
  const provider = walletProvider();
  const walletReady = provider != null;

  useEffect(() => {
    const ac = new AbortController();
    fetchTipTarget(ac.signal)
      .then((t) => {
        if (ac.signal.aborted) return;
        if (!t) {
          setPhase({ kind: "unavailable", reason: "This server has no MANA payment configuration." });
          return;
        }
        setTarget(t);
        setPhase({ kind: "idle" });
      })
      .catch(() => {
        if (!ac.signal.aborted) {
          setPhase({ kind: "unavailable", reason: "Could not load the MANA payment configuration." });
        }
      });
    return () => ac.abort();
  }, []);

  async function send() {
    if (!target || !wei || !provider || self) return;
    setPhase({ kind: "sending" });
    try {
      const from = await connectWallet();
      const hash = await sendManaTip(provider, { from, to: owner, wei, target });
      setPhase({ kind: "sent", hash });
    } catch (e) {
      setPhase({
        kind: "error",
        message: e instanceof Error && e.message ? e.message : "The wallet rejected the transfer.",
      });
    }
  }

  function copyOwner() {
    copyToClipboard(owner);
    setCopied(true);
  }

  const busy = phase.kind === "sending" || phase.kind === "loading";
  const canSend = phase.kind !== "unavailable" && !busy && walletReady && !!target && !!wei && !self;

  return (
    <Modal onClose={onClose} ariaLabel="Send tip to scene owner" className="soa" width={440}>
      <h2 className="soa__title">Send a MANA tip to the scene owner</h2>
      <OwnerMeta sceneTitle={sceneTitle} coords={coords} owner={owner} />
      {phase.kind === "sent" ? (
        <>
          <p className="soa__ok">
            Tip sent on {target ? chainName(target.chainId) : "chain"}. Transaction hash:
          </p>
          <div className="soa__hash">{phase.hash}</div>
          <div className="soa__actions">
            <Button onClick={onClose}>Done</Button>
          </div>
        </>
      ) : (
        <>
          <div className="soa__amounts" role="group" aria-label="Quick amounts">
            {QUICK_AMOUNTS.map((a) => (
              <button
                key={a}
                type="button"
                className="soa__chip"
                aria-pressed={amount === a}
                onClick={() => setAmount(a)}
              >
                {a} MANA
              </button>
            ))}
          </div>
          <label className="soa__amount">
            <span>Amount</span>
            <input
              className="soa__input"
              inputMode="decimal"
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
              aria-label="Tip amount in MANA"
            />
            <span>MANA</span>
          </label>
          {amount.trim() !== "" && !wei && <p className="soa__warn">Enter a positive MANA amount.</p>}
          {phase.kind === "unavailable" && <p className="soa__warn">{phase.reason}</p>}
          {phase.kind === "error" && <p className="soa__warn soa__err">{phase.message}</p>}
          {self && <p className="soa__warn">You own this scene.</p>}
          {phase.kind !== "unavailable" && !walletReady && (
            <div className="soa__fallback">
              <p className="soa__warn">
                No browser wallet found here. Send MANA
                {target ? ` on ${chainName(target.chainId)}` : ""} to the owner from any wallet:
              </p>
              <div className="soa__addr">
                <span>{owner}</span>
                <Button size="sm" variant="secondary" onClick={copyOwner}>
                  {copied ? "Copied" : "Copy address"}
                </Button>
              </div>
            </div>
          )}
          {target && walletReady && (
            <div className="soa__hint">
              Signs an ERC-20 transfer of MANA on {chainName(target.chainId)} from your browser wallet.
            </div>
          )}
          <div className="soa__actions">
            <Button variant="secondary" onClick={onClose}>
              Cancel
            </Button>
            <Button onClick={send} disabled={!canSend}>
              {phase.kind === "sending" ? "Check your wallet\u{2026}" : `Send ${wei ? amount.trim() : ""} MANA`}
            </Button>
          </div>
        </>
      )}
    </Modal>
  );
}
