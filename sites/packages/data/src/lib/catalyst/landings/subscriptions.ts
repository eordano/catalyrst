import { z } from "zod";

import { catalystBase, getJSON, signedGetJSON } from "../client";
import type { GetOptions } from "../client";
import { apiOkOf } from "../envelope";
import { signedFetch } from "../../auth/signer";
import type { AuthIdentity } from "../../auth/types";
import type {
  CommitFn,
  CommitResult,
} from "@features/stories/landings/event-subscriptions/machine";
import { warnInvalid } from "../warn";

/*
 * These are the only values on this surface that get WRITTEN BACK. `PUT
 * /subscription` replaces the whole preference object, so a default here does
 * not just mis-render -- it overwrites the user's real settings with the
 * fabrication. `message_type: {}` in particular would erase every per-type
 * choice they ever made. Everything the commit path reads therefore stays null
 * when unread, and the commit refuses to run rather than write a guess.
 */
export const ChannelSchema = z.object({
  email: z.boolean().nullish().transform((v) => v ?? null),
  in_app: z.boolean().nullish().transform((v) => v ?? null),
});

export const SubscriptionDetailsSchema = z.object({
  ignore_all_email: z.boolean().nullish().transform((v) => v ?? null),
  ignore_all_in_app: z.boolean().nullish().transform((v) => v ?? null),
  message_type: z
    .record(z.string(), ChannelSchema)
    .nullish()
    .transform((v) => v ?? null),
});

export const SubscriptionSchema = z.object({
  address: z.string().nullish().transform((v) => v ?? null),
  email: z.string().nullish().transform((v) => v ?? null),
  details: SubscriptionDetailsSchema,
});
export type Subscription = z.infer<typeof SubscriptionSchema>;

export const GroupSchema = z.object({
  key: z.string(),
  label: z.string(),
  flag: z.string().optional(),
  types: z.array(z.string()),
});
export type Group = z.infer<typeof GroupSchema>;

export const SubscriptionViewSchema = z.object({
  address: z.string(),
  email: z.string(),
  emailConfirmed: z.boolean(),
  subscribed: z.boolean(),
  details: SubscriptionDetailsSchema,
  groups: z.array(GroupSchema),
});
export type SubscriptionView = z.infer<typeof SubscriptionViewSchema>;

export const ProfileSettingsSchema = z
  .object({
    email: z.string().nullish().transform((v) => v ?? null),
    email_verified: z.boolean().nullish().transform((v) => v ?? false),
    notify_by_email: z.boolean().nullish().transform((v) => v ?? false),
    notify_by_browser: z.boolean().nullish().transform((v) => v ?? false),
    subscriptions: z.array(z.unknown()).nullish().transform((v) => v ?? null),
  })
  .passthrough();
export type ProfileSettings = z.infer<typeof ProfileSettingsSchema>;

export async function fetchProfileSettings(
  opts: GetOptions = {},
): Promise<ProfileSettings | null> {
  try {
    const raw = await getJSON<unknown>("/events/api/profiles/me/settings", opts);
    const env = apiOkOf(z.unknown()).safeParse(raw);
    if (!env.success) {
      warnInvalid("ProfileSettings envelope", env.error.issues);
      return null;
    }
    const r = ProfileSettingsSchema.safeParse(env.data.data);
    if (r.success) return r.data;
    warnInvalid("ProfileSettings", r.error.issues);
    return null;
  } catch {
    return null;
  }
}

export type SubscriptionSettingsView = {
  email: string;
  emailConfirmed: boolean;
  subscribed: boolean;
};

export function toSubscriptionSettingsView(
  settings: ProfileSettings,
): SubscriptionSettingsView {
  return {
    email: settings.email ?? "",
    emailConfirmed: settings.email_verified,
    subscribed: settings.notify_by_email,
  };
}

export const NOTIFICATION_GROUPS: Group[] = [
  {
    key: "marketplace",
    label: "Marketplace",
    types: [
      "item_sold",
      "bid_accepted",
      "bid_received",
      "royalties_earned",
      "rental_ended",
      "rental_started",
    ],
  },
  {
    key: "credits",
    label: "Marketplace Credits",
    types: [
      "credits_reminder_complete_goals",
      "credits_reminder_claim_credits",
      "credits_reminder_usage",
      "credits_reminder_do_not_miss_out",
    ],
  },
  { key: "events", label: "Events", types: ["events_started", "events_starts_soon"] },
  {
    key: "rewards",
    label: "Giveaways & Rewards",
    types: [
      "reward_assignment",
      "reward_campaign_out_of_funds",
      "reward_campaign_out_of_stock",
    ],
  },
  {
    key: "dao",
    label: "DAO",
    types: [
      "governance_announcement",
      "governance_authored_proposal_finished",
      "governance_coauthor_requested",
      "governance_new_comment_on_project_update",
      "governance_new_comment_on_proposal",
      "governance_proposal_enacted",
      "governance_voting_ended_voter",
    ],
  },
  {
    key: "worlds",
    label: "Worlds",
    types: [
      "worlds_access_restored",
      "worlds_access_restricted",
      "worlds_missing_resources",
      "worlds_permission_granted",
      "worlds_permission_revoked",
    ],
  },
  {
    key: "streaming",
    label: "In-World Streaming",
    flag: "streaming",
    types: [
      "streaming_key_expired",
      "streaming_key_reset",
      "streaming_key_revoke",
      "streaming_place_updated",
      "streaming_time_exceeded",
    ],
  },
  { key: "tips", label: "Tips", types: ["tip_received"] },
  {
    key: "referral",
    label: "Referrals",
    flag: "referral",
    types: ["referral_invited_users_accepted", "referral_new_tier_reached"],
  },
];

type WriteChannel = { email: boolean; in_app: boolean };

export const UNREADABLE_SETTINGS_MESSAGE =
  "We couldn't read your current notification settings, so nothing was changed. Try again in a moment.";

export function buildSubscriptionCommit(
  identity: AuthIdentity | null,
): CommitFn {
  return async ({ kind, enabledTypes, signal }): Promise<CommitResult> => {
    if (!identity) throw new Error("Sign in to manage email notifications.");

    const current = await signedGetJSON<unknown>("/subscription", {
      identity,
      signal,
    });
    const parsed = SubscriptionSchema.safeParse(current);
    if (!parsed.success) {
      throw new Error(UNREADABLE_SETTINGS_MESSAGE);
    }
    const details = parsed.data.details;
    if (details.message_type === null || details.ignore_all_in_app === null) {
      throw new Error(UNREADABLE_SETTINGS_MESSAGE);
    }

    // Every surviving type is copied verbatim. A channel with an unread flag
    // aborts the write: this PUT replaces the whole preference object, so
    // guessing one flag would silently rewrite a setting the user chose.
    const messageType: Record<string, WriteChannel> = {};
    for (const [type, chan] of Object.entries(details.message_type)) {
      if (chan.email === null || chan.in_app === null) {
        throw new Error(UNREADABLE_SETTINGS_MESSAGE);
      }
      messageType[type] = { email: chan.email, in_app: chan.in_app };
    }

    const subscribing = kind === "subscribe";
    if (subscribing) {
      for (const type of enabledTypes) {
        const chan = messageType[type] ?? { email: false, in_app: false };
        messageType[type] = { email: true, in_app: chan.in_app };
      }
    }

    const body = {
      ignore_all_email: !subscribing,
      ignore_all_in_app: details.ignore_all_in_app,
      message_type: messageType,
    };

    const res = await signedFetch(identity, `${catalystBase()}/subscription`, {
      method: "PUT",
      metadata: {},
      headers: { accept: "application/json", "content-type": "application/json" },
      body: JSON.stringify(body),
      signal,
    });
    if (!res.ok) {
      throw new Error(`Subscription update returned ${res.status}.`);
    }

    return { kind, at: Date.now() };
  };
}
