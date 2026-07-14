import { readSid } from "@core/lib/experiments/assign";

import { canonicalSid } from "@data/lib/foundry/db.server";
import { readGateMemo, writeGateMemo } from "@data/lib/foundry/gate-memo.server";
import { activeRoles } from "@data/lib/foundry/roles.server";

import type { Route } from "./+types/foundry.copilot-gate";

// Resource route behind nginx's auth_request on copilot.catalyst.example.com: 204 admits,
// 401 refuses. The copilot executes code on the operator host, so the bar is a
// privileged role — admin or host — held by this browser's session; those are
// only ever granted through one-time invites, never self-chosen. The check is
// principal-aware: the sid resolves through the alias layer, so a role that
// landed under an earlier sid of the same persona still opens the door.
//
// Verdicts are memoised per CANONICAL sid (gate-memo.server), and a return-code
// redeem or an operator rebind clears the memo, so a rebind is honored within
// the memo window instead of after it.

export async function loader({ request }: Route.LoaderArgs) {
  const sid = readSid(request);
  if (!sid) return new Response(null, { status: 401 });

  let canon = sid;
  try {
    canon = await canonicalSid(sid);
  } catch {
    // unreadable alias layer degrades to the raw sid, same as no alias
  }

  const memo = readGateMemo(canon);
  if (memo !== null) {
    return new Response(null, { status: memo ? 204 : 401 });
  }

  let ok = false;
  try {
    const roles = await activeRoles(canon);
    ok = roles.includes("admin") || roles.includes("host");
  } catch {
    ok = false;
  }

  writeGateMemo(canon, ok);
  return new Response(null, { status: ok ? 204 : 401 });
}
