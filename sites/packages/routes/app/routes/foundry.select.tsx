import { useEffect, useRef } from "react";

import FdSelectPage, {
  type FdSelectNextSession,
} from "@ui/foundry/pages/FdSelectPage";
import { useChromeAuth } from "@ui/web/frames/chrome-auth";

import "@ui/atoms/button.css";
import "@ui/explorer/components/avatarpreview.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdavatarcrowd.css";
import "@ui/foundry/pages/fdselect.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { FoundryUnavailableError, sidBadge } from "@data/lib/foundry/db.server";
import { activeRoles } from "@data/lib/foundry/roles.server";
import { listUpcoming } from "@data/lib/foundry/sessions.server";

import type { Route } from "./+types/foundry.select";

export function meta() {
  return [{ title: "Start here — The Foundry" }];
}

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);
  // With no database the calendar stays unread: the page says so rather than
  // rendering an empty calendar it never looked at.
  let heldRoles: string[] = [];
  let nextSession: FdSelectNextSession | null | undefined;
  try {
    const [occurrences, roles] = await Promise.all([
      listUpcoming(sid),
      activeRoles(sid),
    ]);
    heldRoles = roles;
    const next = occurrences[0];
    nextSession = next
      ? {
          title: next.title,
          occurrenceAt: next.occurrenceAt,
          rsvpCount: next.rsvpCount,
        }
      : null;
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
  }
  return wrap({ badge: sidBadge(sid), heldRoles, nextSession });
}

export default function FoundrySelect({ loaderData }: Route.ComponentProps) {
  const ctx = { sid: loaderData.badge };
  const auth = useChromeAuth();
  const viewer =
    auth.signedIn && auth.account
      ? { address: auth.account, name: auth.name }
      : null;

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    // The doors moved to /foundry; what this page shows of roles is the ones
    // this session holds.
    track("fd_select_viewed", { roles_shown: loaderData.heldRoles.length }, ctx);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loaderData.badge]);

  return (
    <FdSelectPage
      viewer={viewer}
      heldRoles={loaderData.heldRoles}
      nextSession={loaderData.nextSession}
    />
  );
}
