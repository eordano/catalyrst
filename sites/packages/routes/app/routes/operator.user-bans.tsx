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
