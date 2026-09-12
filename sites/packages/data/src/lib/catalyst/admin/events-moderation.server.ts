
import { controlStatus } from "./control-availability";
import type { Unavailable } from "./availability";

export type PendingEventQueue = never;

export function loadEventModerationQueue(): Unavailable {
  return controlStatus("events.moderation.queue") as Unavailable;
}

export function commitEventModeration(): Unavailable {
  return controlStatus("events.moderation.decide") as Unavailable;
}
