import { useCallback, useEffect, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { Link, useSearchParams } from "react-router";

import Button from "@ui/atoms/Button";
import SitesChrome from "@ui/web/frames/SitesChrome";
import "@ui/admin/admin.css";

import { useAuth } from "@data/lib/auth/index";
import type { TrackContext } from "@core/lib/telemetry/track";
import {
  commitUserAction,
  loadActiveBans,
  shortAddress,
  type UserAction,
  type UserBan,
} from "@data/lib/catalyst/admin/user-bans";
import UserBanTable from "../../../components/operator/UserBanTable";
import BanStatusLookup from "../../../components/operator/BanStatusLookup";
import UserBanActionForm from "../../../components/operator/UserBanActionForm";
import {
  userBanMachine,
  resolveUserBanSnapshot,
  slugToState,
  stateToSlug,
  type CommitFn,
  type TrackFn,
} from "./machine";

type UserBanWizardProps = {
  trackCtx: TrackContext;
  bans: UserBan[];
  bansFallback?: boolean;
  moderator: string;
  initialStep?: string;
  commit?: CommitFn;
  track?: TrackFn;
};

const OPERATOR_NAV = [
  { to: "/operator/dashboard", label: "Dashboard" },
  { to: "/operator/scene-bans", label: "Scene bans" },
  { to: "/operator/scene-admins", label: "Scene admins" },
] as const;

export default function UserBanWizard({
  trackCtx,
  bans,
  bansFallback = false,
  moderator,
  initialStep,
  commit,
  track,
}: UserBanWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <SitesChrome active="play">
      <div className="adm">
        <div className="adm__page">
          <div className="adm__inner adm__inner--mid">
            <h1 className="adm__title">User bans &amp; warnings</h1>
            <nav className="adm-pills" aria-label="Operator tools">
              {OPERATOR_NAV.map((item) => (
                <Link key={item.to} to={item.to} prefetch="intent" className="adm-pill">
                  {item.label}
                </Link>
              ))}
            </nav>
            <UserBanWizardInner
              key={stateId}
              stateId={stateId}
              trackCtx={trackCtx}
              bans={bans}
              bansFallback={bansFallback}
              moderator={moderator}
              commit={commit}
              track={track}
            />
          </div>
        </div>
      </div>
    </SitesChrome>
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  trackCtx: TrackContext;
  bans: UserBan[];
  bansFallback: boolean;
  moderator: string;
  commit?: CommitFn;
  track?: TrackFn;
};

function UserBanWizardInner({ stateId, trackCtx, bans, bansFallback, moderator, commit, track }: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const auth = useAuth();
  const [liveBans, setLiveBans] = useState<UserBan[] | null>(null);

  const identity = auth.identity;
  useEffect(() => {
    if (!identity) return;
    let cancelled = false;
    loadActiveBans({ identity })
      .then((rows) => {
        if (!cancelled) setLiveBans(rows);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [identity]);

  const shownBans = liveBans ?? bans;
  const activeAddresses = shownBans.map((b) => b.bannedAddress);
  const moderatorLabel = auth.address ?? moderator;

  const identityRef = useRef(identity);
  useEffect(() => {
    identityRef.current = identity;
  }, [identity]);

  const liveCommit = useCallback<CommitFn>(
    ({ action, address, reason, durationMs, customMessage, signal }) =>
      commitUserAction({
        identity: identityRef.current,
        action,
        address,
        reason,
        durationMs,
        customMessage,
        signal,
      }),
    [],
  );
  const resolvedCommit = commit ?? liveCommit;

  const snapshot = useRef(
    resolveUserBanSnapshot({
      step: stateId,
      trackCtx,
      moderator: moderatorLabel,
      activeAddresses,
      commit: resolvedCommit,
      track,
      address: activeAddresses[0],
    }),
  ).current;

  const [state, send] = useMachine(userBanMachine, {
    input: {
      trackCtx,
      moderator: moderatorLabel,
      activeAddresses,
      commit: resolvedCommit,
      track,
    },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const ctx = state.context;

  const lastStep = useRef<string | null>(null);
  useEffect(() => {
    if (lastStep.current === step) return;
    lastStep.current = step;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        if (params.get("step") === step) return params;
        params.set("step", step);
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, setSearchParams]);

  const address = ctx.address ?? "";
  const targetIsBanned =
    ctx.lookupIsBanned ??
    activeAddresses.map((a) => a.toLowerCase()).includes(address.toLowerCase());

  return (
    <div className="adm-stack adm-stack--lg" data-step={step}>
      {value === "authGate" && (
        <div className="adm-card" role="region" aria-label="Moderator sign-in">
          <p className="adm-card__text">
            Every commit is signed with your wallet and checked against the comms
            moderator allowlist. {auth.isConnected
              ? `Signing as ${shortAddress(auth.address ?? "")}.`
              : "Connect the wallet that is on the allowlist \u{2014} commits fail without it."}
          </p>
          <div className="adm-actions">
            {!auth.isConnected && (
              <Button
                variant="secondary"
                onClick={() => {
                  void auth.connect();
                }}
              >
                Connect wallet
              </Button>
            )}
            <Button onClick={() => send({ type: "SIGN_IN" })}>Continue to moderation</Button>
          </div>
        </div>
      )}

      {value === "bans" && (
        <>
          <BanStatusLookup
            onLookup={({ address: a, isBanned }) =>
              send({ type: "LOOKUP", address: a, isBanned })
            }
            onAct={({ address: a, isBanned }) =>
              send({
                type: "SELECT",
                action: (isBanned ? "unban" : "ban") as UserAction,
                address: a,
              })
            }
          />
          {bansFallback && !liveBans && (
            <p className="adm-dim" role="status">
              The global ban list needs a signed moderator session. Connect an
              allowlisted wallet to load it &#x2014; per-address lookups above are public.
            </p>
          )}
          <UserBanTable
            bans={shownBans}
            onLift={(a) => send({ type: "SELECT", action: "unban", address: a })}
            onSelect={(a) => send({ type: "SELECT", action: "ban", address: a })}
          />
        </>
      )}

      {value === "action" && (
        <UserBanActionForm
          address={address}
          isBanned={targetIsBanned}
          error={ctx.error ?? null}
          onBack={() => send({ type: "BACK" })}
          onSubmit={(s) => {
            send({
              type: "SELECT",
              action: s.action,
              address: s.address,
              reason: s.reason,
              durationMs: s.durationMs,
              customMessage: s.customMessage,
            });
            send({ type: "REVIEW" });
          }}
        />
      )}

      {value === "confirm" && (
        <div className="adm-card" role="region" aria-label="Confirm action">
          <h2 className="adm__h2">Confirm {ctx.action}</h2>
          <dl className="adm-stats adm-stats--grid">
            <dt>Address</dt>
            <dd>{shortAddress(address)}</dd>
            <dt>Action</dt>
            <dd>{ctx.action}</dd>
            {ctx.action !== "unban" && (
              <>
                <dt>Reason</dt>
                <dd>{ctx.reason}</dd>
              </>
            )}
            {ctx.action === "ban" && (
              <>
                <dt>Duration</dt>
                <dd>{ctx.durationMs ? `${Math.round(ctx.durationMs / 3600000)}h` : "Permanent"}</dd>
                {ctx.customMessage && (
                  <>
                    <dt>Message</dt>
                    <dd>{ctx.customMessage}</dd>
                  </>
                )}
              </>
            )}
          </dl>
          {ctx.action === "ban" && (
            <p className="adm-card__text adm-dim">
              Committing a ban also disconnects this address from every LiveKit room.
            </p>
          )}
          <div className="adm-actions">
            <Button variant="ghost" onClick={() => send({ type: "CANCEL" })}>
              Back
            </Button>
            <Button tone={ctx.action === "unban" ? "success" : "danger"} onClick={() => send({ type: "COMMIT" })}>
              Commit
            </Button>
          </div>
        </div>
      )}

      {value === "submitting" && (
        <div className="adm-card" role="status" aria-live="polite">
          <p className="adm-card__title">Applying moderation&#x2026;</p>
          <p className="adm-card__text adm-dim">
            Committing {ctx.action} on {shortAddress(address)} as {moderatorLabel}.
          </p>
        </div>
      )}

      {value === "done" && (
        <div className="adm-card" role="status" aria-live="polite">
          <div className="adm-notice" data-tone="ok">
            <span>
              {ctx.action === "ban" && `Banned ${shortAddress(address)} and dropped their rooms.`}
              {ctx.action === "warn" && `Warned ${shortAddress(address)}.`}
              {ctx.action === "unban" && `Lifted the ban on ${shortAddress(address)}.`}
            </span>
          </div>
          <div className="adm-actions adm-actions--start">
            <Button onClick={() => send({ type: "CONTINUE" })}>Continue</Button>
          </div>
        </div>
      )}
    </div>
  );
}
