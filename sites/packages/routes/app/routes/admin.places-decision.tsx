import type { ActionFunctionArgs } from "react-router";

/**
 * Resource route for committing a places moderation decision.
 *
 * This exists so the browser never holds `PLACES_ADMIN_AUTH_TOKEN`. The page at
 * `/admin/places-moderation` posts here; only this action -- running on the
 * server -- imports `places-moderation.server.ts`, which is where the bearer is
 * read. Same shape as `/admin/community-suspension`.
 *
 * Server-side authorization, read in this session:
 *   catalyrst/crates/catalyrst-places/src/handlers/admin.rs:13-15  `gate()`
 *     -> catalyrst/crates/catalyrst-places/src/auth.rs:88-100  `require_admin_bearer`
 *        `expected: None`         -> 403 "Admin token not configured"
 *        bearer absent / mismatch -> 403 "Invalid admin credentials"
 *        the compare at auth.rs:77-86 is timing-safe.
 *   `gate()` is the first statement of `patch_report` (admin.rs:83) and of
 *   `patch_place_disable` (admin.rs:131).
 *
 * Fails closed: with no token configured, `commitModerationDecision` answers
 * 503 "not-configured" before any request leaves this process.
 */
function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

/**
 * DISABLED -- this route is a confused deputy and must not be enabled until it
 * authenticates its own caller.
 *
 * Moving `PLACES_ADMIN_AUTH_TOKEN` out of the browser bundle was correct and is
 * kept. But the token moving server-side is only half the job: this action ran
 * no check of WHO was calling -- no session, no wallet signature, no origin --
 * and then attached `authorization: Bearer <PLACES_ADMIN_AUTH_TOKEN>`
 * (places-moderation.server.ts:106-109) to `PATCH /places/api/reports/{id}` and
 * `PATCH /places/api/places/{id}/disable`.
 *
 * The places server's own gate (catalyrst-places/src/auth.rs:88-100) checks the
 * bearer, which this process supplies, so it would have accepted every call.
 * Net effect:
 *
 *     curl -X POST https://<host>/admin/places-decision \
 *       -d '{"reportId":"...","decision":"action","disablePlace":true}'
 *
 * would disable an arbitrary place for any anonymous caller who can reach this
 * app. The previous client-side implementation was broken but harmless -- it
 * sent an UNAUTHENTICATED PATCH the places server always rejected. Authenticating
 * it without authenticating the caller turned a cosmetic bug into privilege
 * escalation, which is strictly worse.
 *
 * Failing closed here restores the old safety without restoring the old lie:
 * the UI renders an honest unavailable state rather than a control that appears
 * to work.
 *
 * To enable, this route needs a real caller check. This app has no session
 * layer today, so the honest options are:
 *   a) require an ADR-44 signed request from the caller and forward that
 *      identity, letting the places server decide -- no shared bearer at all; or
 *   b) gate on a caller credential that actually exists and is verified here
 *      before the bearer is ever attached.
 * Hiding the endpoint or checking a Referer is not a caller check.
 */
const CALLER_IS_NOT_AUTHENTICATED =
  "Places moderation writes are disabled: this endpoint does not yet verify who is calling, " +
  "and it holds a privileged token. Enabling it without a caller check would let any " +
  "anonymous request act as the places administrator.";

export async function action({ request }: ActionFunctionArgs) {
  if (request.method !== "POST") {
    return json({ error: "Method not allowed." }, 405);
  }

  return json(
    {
      error: CALLER_IS_NOT_AUTHENTICATED,
      reason: "caller-not-authenticated",
      serverCheck: "catalyrst-places/src/auth.rs:88-100 (reachable, but by anyone)",
    },
    503,
  );
}
