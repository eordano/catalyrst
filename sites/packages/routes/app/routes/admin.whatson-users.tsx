import { useEffect, useRef } from "react";

import "@ui/web/pages/stwhatsonadminusers.css";

import AdControlNotice, { AdBlockedAction } from "@ui/admin/pages/AdControlNotice";
import SitesChrome from "@ui/web/frames/SitesChrome";

import { loadAdminUsers } from "@data/lib/catalyst/admin/whatson-admin-users";
import { controlStatus } from "@data/lib/catalyst/admin/control-availability";
import type { Unavailable } from "@data/lib/catalyst/admin/availability";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track, type TrackContext } from "@core/lib/telemetry/track";

import type { Route } from "./+types/admin.whatson-users";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "admin/whatson-users";

const FALLBACK: Assignment = {
  variant: "users_table",
  flags: { users_table: true },
  experimentKey: "admin_whatson_users",
};

/**
 * `?admin=1` is gone.
 *
 * The loader used to turn `?admin=1` into a `hasAdminScope` boolean handed to
 * the data layer -- a query parameter the visitor sets, standing in for an
 * authorization decision. That is exactly the pretend-gate this build removes.
 *
 * The real gate is server-side and needs no secret, only the caller's wallet:
 *   catalyrst-events/src/handlers/profile_settings.rs:40-41
 *     `require_auth(&headers, "get", "/api/profiles/settings")`
 *     then `authority::require_moderator`
 *   catalyrst-fed/src/authority.rs:5-11  SELECT 1 FROM moderators WHERE address=$1
 *   catalyrst-fed/src/authority.rs:13-19 403 otherwise
 *
 * When the read path is fixed, this page will show what that server answers to
 * the connected wallet -- 200 (a moderator row exists), 403 (you are not a
 * moderator on this node), 401 (not connected) -- and nothing else. It will not
 * simulate any of those answers locally.
 *
 * Until then `loadAdminUsers` reports the three named client-side defects
 * instead of fetching, and this page renders that reason. The previous
 * behaviour swallowed a 403 into `{ rows: [], source: "empty" }`, so "you are
 * not allowed to see this" and "there are no moderators" rendered identically.
 *
 * Saving permissions is a separate BLOCK (`profile_settings.rs:145-155`): the
 * gate is real, and nothing on this side can reach it -- the signed federation
 * envelope the endpoint requires has no builder in this repo. It renders as a
 * disabled control with that reason, not as a save button that cannot save.
 */
export async function loader({ request }: Route.LoaderArgs) {
  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const users = await loadAdminUsers();
  const save = controlStatus("whatson.users.savePermissions") as Unavailable;

  const payload = {
    sid,
    read: users.ok
      ? null
      : {
          reason: users.reason,
          status: users.status,
          message: users.message,
          serverCheck: users.serverCheck,
          fix: users.fix,
        },
    save: {
      reason: save.reason,
      message: save.message,
      serverCheck: save.serverCheck,
      fix: save.fix,
    },
    assignment,
  };

  return wrap(payload);
}

export default function AdminWhatsOnUsersRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const trackCtx: TrackContext = {
    sid: d.sid,
    story: STORY,
    variant: d.assignment.variant,
    experimentKey: d.assignment.experimentKey,
  };

  useUnavailableViewed(trackCtx, d.read?.reason ?? "available");

  return (
    <SitesChrome active="play">
      <main className="admin-whatson-users-route">
        <h1>What&apos;s On moderators</h1>

        {d.read && (
          <AdControlNotice
            title="The moderator list cannot be read on this node"
            message={d.read.message}
            status={d.read.status}
            serverCheck={d.read.serverCheck}
            fix={d.read.fix}
          />
        )}

        <AdControlNotice
          title="Editing moderator permissions"
          message={d.save.message}
          serverCheck={d.save.serverCheck}
          fix={d.save.fix}
        />
        <AdBlockedAction
          label="Save permissions"
          reason={d.save.message}
        />
      </main>
    </SitesChrome>
  );
}

function useUnavailableViewed(trackCtx: TrackContext, reason: string): void {
  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    track("admin_users_unavailable_viewed", { reason }, trackCtx);
  }, [trackCtx, reason]);
}
