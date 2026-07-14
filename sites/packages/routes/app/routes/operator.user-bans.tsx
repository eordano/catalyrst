import { useEffect, useRef } from "react";

import AdControlNotice, { AdBlockedAction } from "@ui/admin/pages/AdControlNotice";
import SitesChrome from "@ui/web/frames/SitesChrome";

import { controlStatus } from "@data/lib/catalyst/admin/control-availability";
import type { Unavailable } from "@data/lib/catalyst/admin/availability";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track, type TrackContext } from "@core/lib/telemetry/track";

import type { Route } from "./+types/operator.user-bans";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "admin/operator-user-bans";

const FALLBACK: Assignment = {
  variant: "console",
  flags: { console: true },
  experimentKey: "op_user_bans_console",
};

/**
 * Platform user bans -- BLOCK, for a reason worth stating precisely.
 *
 * This is the one blocked surface whose gate is both real and genuinely
 * satisfiable by a human operator without any secret:
 *
 *   catalyrst-comms/src/handlers/user_bans.rs:241  (list)
 *   catalyrst-comms/src/handlers/user_bans.rs:95-102, :164-171, :212-219
 *     (ban / unban / warn)
 *   -> catalyrst-comms/src/moderator.rs:65-116 `authorize_moderator`:
 *      a `MODERATOR_TOKEN` bearer, OR a signer present in `moderator_addresses`
 *      (and it explicitly rejects the scene signer).
 *
 * The wallet allowlist is populated on this node
 * (`deploy/env/catalyrst-comms.env:51`), so the gate is live and
 * wallet-satisfiable. What is broken is on this side: `user-bans.ts:135-139`
 * and `:233-263` sign the un-prefixed `/users/0x.../bans`, while nginx sets
 * `x-original-path` on `/comms/` (`01-catalyst.conf:127`) and
 * `catalyrst-crypto/src/signed_fetch.rs:119-134` verifies the *prefixed* value
 * -- proven by that crate's own test at `:671-687`. Every request 401s.
 *
 * The fix is named and small (sign `${COMMS_PREFIX}${path}` when the base is
 * the nginx edge), but the build gate requires it to be observed returning 200
 * from a `PLATFORM_USER_MODERATORS` wallet before it ships, and that runtime
 * check has not been done. So this stays BLOCK and is not wired.
 *
 * What is NOT done to "solve" this: putting `MODERATOR_TOKEN` in `sites.env`.
 * It is a server-to-server bearer; the wallet allowlist is the right gate for a
 * human operator and needs no secret at all.
 *
 * The loader therefore does not call `loadActiveBans`. Calling it produced a
 * 401 that the route swallowed into an empty list -- "no one is banned" and "you
 * cannot see who is banned" rendered identically.
 */
export async function loader({ request }: Route.LoaderArgs) {
  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const list = controlStatus("userBans.list") as Unavailable;
  const ban = controlStatus("userBans.ban") as Unavailable;
  const unban = controlStatus("userBans.unban") as Unavailable;
  const warn = controlStatus("userBans.warn") as Unavailable;

  const payload = {
    sid,
    list: {
      status: list.status,
      message: list.message,
      serverCheck: list.serverCheck,
      fix: list.fix,
    },
    actions: [
      { label: "Ban user", reason: ban.message },
      { label: "Unban user", reason: unban.message },
      { label: "Warn user", reason: warn.message },
    ],
    assignment,
  };

  return wrap(payload);
}

export default function OperatorUserBansRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  const ctx: TrackContext = {
    sid: d.sid,
    story: STORY,
    variant: d.assignment.variant,
    experimentKey: d.assignment.experimentKey,
  };

  useUnavailableViewed(ctx, d.list.message);

  return (
    <SitesChrome active="create">
      <main className="operator-user-bans-route">
        <h1>Platform user bans</h1>

        <AdControlNotice
          title="The active ban list cannot be read on this node"
          message={d.list.message}
          status={d.list.status}
          serverCheck={d.list.serverCheck}
          fix={d.list.fix}
        />

        <div className="sa__toolbar">
          {d.actions.map((a) => (
            <AdBlockedAction key={a.label} label={a.label} reason={a.reason} />
          ))}
        </div>
      </main>
    </SitesChrome>
  );
}

function useUnavailableViewed(ctx: TrackContext, reason: string) {
  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    track(
      "operator_control_unavailable",
      { control: "userBans.list", reason },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}
