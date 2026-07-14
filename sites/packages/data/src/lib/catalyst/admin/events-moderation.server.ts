/**
 * Event moderation -- deliberately NOT wired.
 *
 * The server-side gate is real and fails closed:
 *   catalyrst-events/src/handlers/events.rs:512  get_moderation_list
 *   catalyrst-events/src/handlers/events.rs:657  patch_event
 *     both call `crate::admin::authorize_admin` as their first statement
 *     -> catalyrst-events/src/admin.rs:34-44
 *        token `None`      -> 403 "Admin operations are disabled"
 *        bearer mismatch   -> 403 "You are not authorized to access this resource"
 *        compare at :25-32 is timing-safe.
 *
 * The credential is `CATALYRST_EVENTS_ADMIN_TOKEN`
 * (catalyrst-events/src/config.rs:27). It is set in no env file in this repo.
 *
 * Making this work needs a bearer-holding server module *and* a provisioned
 * token. The build gate classifies both as FIX-FIRST -- the code is not written
 * here, and the token is not provisioned here. Until both land, the controls
 * render as explicitly unavailable rather than as buttons waiting for a secret.
 *
 * In particular this replaces the previous default of `simulateModerateAction`
 * (features/src/stories/landings/whatson-admin-moderate/machine.ts:169,263),
 * which returned success for a decision that never left the browser. An
 * unavailable state is the honest substitute; a fake success is not.
 */

import { controlStatus } from "./control-availability";
import type { Unavailable } from "./availability";

export type PendingEventQueue = never;

export function loadEventModerationQueue(): Unavailable {
  return controlStatus("events.moderation.queue") as Unavailable;
}

export function commitEventModeration(): Unavailable {
  return controlStatus("events.moderation.decide") as Unavailable;
}
