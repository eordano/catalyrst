import { redirect } from "react-router";

import { readSid } from "@core/lib/experiments/assign";

import {
  copilotPublicUrl,
  copilotSessionUrl,
  createCopilotDoorSession,
} from "@data/lib/foundry/copilot.server";
import { canonicalSid } from "@data/lib/foundry/db.server";
import { getPersona } from "@data/lib/foundry/persona.server";
import { activeRoles } from "@data/lib/foundry/roles.server";

import type { Route } from "./+types/foundry.copilot.enter";

// The door itself. The copilot web UI opens on a per-browser recents list and
// a folder picker that cannot reach the workspace, so a bare link strands
// first-time visitors in a project with none of the workspace commands or the
// model route. This route mints one workspace session for the visitor and
// sends them straight into it; the same admin-or-host bar as the nginx
// auth_request applies, so an unprivileged session never mints anything.

export async function loader({ request }: Route.LoaderArgs) {
  const base = copilotPublicUrl();
  if (!base) return redirect("/foundry/copilot");

  const sid = readSid(request);
  if (!sid) return redirect("/foundry/copilot");

  let canon = sid;
  try {
    canon = await canonicalSid(sid);
  } catch {
    // unreadable alias layer degrades to the raw sid, same as the gate
  }
  let allowed = false;
  try {
    const roles = await activeRoles(canon);
    allowed = roles.includes("admin") || roles.includes("host");
  } catch {
    allowed = false;
  }
  if (!allowed) return redirect("/foundry/copilot");

  try {
    const persona = await getPersona(canon);
    const who = persona?.displayName ?? `visitor ${canon.slice(0, 4)}`;
    const id = await createCopilotDoorSession(`${who} — from the site`);
    const url = copilotSessionUrl(id);
    if (url) return redirect(url);
  } catch {
    // the copilot may be down or mute; the plain door still exists
  }
  return redirect(base);
}
