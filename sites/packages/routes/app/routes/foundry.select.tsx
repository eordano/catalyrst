import { useEffect, useRef } from "react";

import FdSelectPage, {
  FD_ROLES,
  type FdRoleId,
} from "@ui/foundry/pages/FdSelectPage";
import { useChromeAuth } from "@ui/web/frames/chrome-auth";

import "@ui/explorer/components/avatarpreview.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdavatarcrowd.css";
import "@ui/foundry/components/fdrolecard.css";
import "@ui/foundry/pages/fdselect.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { sidBadge } from "@data/lib/foundry/db.server";

import type { Route } from "./+types/foundry.select";

// The door a visitor picked, so a later first-run check can skip this page
// instead of asking again. It is a preference, not a permission: every surface
// stays reachable whatever is in here.
const ROLE_KEY = "fd_select_role";

export function meta() {
  return [{ title: "Choose how you start — The Foundry" }];
}

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);
  return wrap({ badge: sidBadge(sid) });
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
    track("fd_select_viewed", { roles_shown: FD_ROLES.length }, ctx);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loaderData.badge]);

  function onChoose(id: FdRoleId) {
    const door = FD_ROLES.find((r) => r.id === id);
    if (!door) return;
    try {
      localStorage.setItem(ROLE_KEY, id);
    } catch {
      // A blocked storage bucket loses the preference, not the navigation.
    }
    // `role` is the door the visitor picked; `destination` is where it leads.
    // An `intent` that only ever echoed `role` measured nothing, so it is gone.
    track(
      "fd_role_chosen",
      { role: id, destination: door.destinationId },
      ctx,
    );
  }

  return <FdSelectPage viewer={viewer} onChoose={onChoose} />;
}
