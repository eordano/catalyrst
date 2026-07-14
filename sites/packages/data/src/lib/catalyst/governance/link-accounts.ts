import { z } from "zod";

import fixture from "../../../fixtures/governance-link-accounts.json";

export const PROVIDERS = ["forum", "discord", "push"] as const;
export type Provider = (typeof PROVIDERS)[number];

export function toProvider(raw: string | null | undefined): Provider | null {
  const v = raw?.trim().toLowerCase();
  return v === "forum" || v === "discord" || v === "push" ? (v as Provider) : null;
}

const StepSchema = z.object({
  title: z.string(),
  description: z.string(),
  action: z.string(),
  helper: z.string(),
  card_helper_start: z.string().nullish(),
  card_helper_active: z.string().nullish(),
  card_helper_success: z.string().nullish(),
  card_helper_error: z.string().nullish(),
});

const AccountSchema = z.object({
  id: z.enum(PROVIDERS),
  kind: z.enum(["steps", "subscribe"]),
  card_title: z.string(),
  card_description: z.string(),
  title: z.string(),
  subtitle: z.string().nullish(),
  helper: z.string().nullish(),
  confirm_label: z.string(),
  helper_loading: z.string().nullish(),
  subscribing_title: z.string().nullish(),
  subscribing_body: z.string().nullish(),
  steps: z.array(StepSchema).optional(),
  success_text: z.string(),
  success_body: z.string(),
  success_subtext: z.string(),
  success_button: z.string(),
  error_text: z.string(),
  error_body: z.string(),
  error_subtext: z.string(),
  error_button: z.string(),
});

const LinkAccountsSchema = z.object({
  title: z.string(),
  description: z.string(),
  timer: z.string(),
  timer_expired: z.string(),
  verified_label: z.string(),
  soon_label: z.string(),
  providers: z.array(z.enum(PROVIDERS)),
  accounts: z.object({
    forum: AccountSchema,
    discord: AccountSchema,
    push: AccountSchema,
  }),
  unlink: z.object({
    title: z.string(),
    body: z.string(),
    confirm_button: z.string(),
    cancel_button: z.string(),
    note: z.string(),
  }),
  verification_seconds: z.number(),
});

export type LinkStep = z.infer<typeof StepSchema>;
export type AccountCopy = z.infer<typeof AccountSchema>;
export type LinkAccountsData = z.infer<typeof LinkAccountsSchema>;

function fallbackSteps(post: string): LinkStep[] {
  return [
    {
      title: "1. Sign message",
      description: "Use your web3 wallet to securely sign a message to start the linking process.",
      action: "Sign",
      helper: "First, sign the message.",
      card_helper_start: "Your web3 wallet will trigger a signing request",
    },
    {
      title: "2. Copy to clipboard",
      description: "You'll be using the same content as the signature for the next step.",
      action: "Copy",
      helper: "Then, copy the signed message.",
      card_helper_start: "Signature required",
    },
    {
      title: post,
      description: "Paste the content to complete the linking.",
      action: "Open",
      helper: "Finally, post the signed message.",
    },
  ];
}

const FALLBACK: LinkAccountsData = {
  title: "Link your Decentraland profile to external services",
  description:
    "By linking your Decentraland profile to your other social accounts you will be able to enhance your governance experience.",
  timer: "Time sensitive task. {time} left to complete",
  timer_expired: "Signature expired. You'll have to start over.",
  verified_label: "Verified",
  soon_label: "Soon",
  providers: [...PROVIDERS],
  accounts: {
    forum: {
      id: "forum",
      kind: "steps",
      card_title: "Decentraland Forum",
      card_description: "Publish comments on proposals using your Decentraland profile.",
      title: "Decentraland Forum Account",
      confirm_label: "Confirm link",
      steps: fallbackSteps("3. Post on Forum thread"),
      success_text: "Decentraland and Forum accounts linked successfully \u{1F389}",
      success_body: "Now the comments you left on Proposals will link to your Decentraland profile.",
      success_subtext: "Plus, you'll get a fancy icon next to your username",
      success_button: "Take me to my Profile",
      error_text: "Governance and Forum accounts could not be linked",
      error_body: "Something happened.",
      error_subtext: "Maybe just have a go at it one more time?",
      error_button: "Retry",
    },
    discord: {
      id: "discord",
      kind: "steps",
      card_title: "Discord",
      card_description: "Receive real-time notifications customized to your activity.",
      title: "Discord Account",
      confirm_label: "Confirm link",
      steps: fallbackSteps("3. Post on Discord channel"),
      success_text: "Discord and Governance accounts linked successfully \u{1F389}",
      success_body: "You'll start getting notified via Discord DM.",
      success_subtext: " ",
      success_button: "Take me to my Profile",
      error_text: "Discord Account linking failed",
      error_body: "Something happened.",
      error_subtext: "Maybe have a go at it one more time?",
      error_button: "Retry",
    },
    push: {
      id: "push",
      kind: "subscribe",
      card_title: "Push Protocol Notifications",
      card_description: "Receive notifications in your wallet using this native web3 messaging protocol.",
      title: "Push Protocol Notifications",
      helper: "TRIGGERS MESSAGE SIGN REQUEST",
      confirm_label: "Subscribe",
      subscribing_title: "Subscribing to Push notifications\u{2026}",
      subscribing_body: "Confirm the signature request in your wallet to enable Governance notifications.",
      steps: [],
      success_text: "Push Notifications enabled successfully!",
      success_body: "",
      success_subtext: "Now you will receive Governance-related notifications in your Push account.",
      success_button: "Connect other accounts",
      error_text: "Couldn't subscribe",
      error_body: "Something went wrong while subscribing. Please try again.",
      error_subtext: "",
      error_button: "Retry",
    },
  },
  unlink: {
    title: "Unlink Confirmation",
    body: "Are you sure you want to unlink this account?",
    confirm_button: "Unlink",
    cancel_button: "Cancel",
    note: "Account linking is not available yet \u{2014} no account can be linked or unlinked from this page.",
  },
  verification_seconds: 278,
};

function parse(): LinkAccountsData {
  const parsed = LinkAccountsSchema.safeParse(fixture);
  return parsed.success ? parsed.data : FALLBACK;
}

export function getLinkAccountsData(): LinkAccountsData {
  return parse();
}

export type VerifyResult = { provider: Provider; verified: true };

export type VerifyAccountFn = (args: {
  provider: Provider;
  signal?: AbortSignal;
}) => Promise<VerifyResult>;

export const PROVIDER_BLOCKER: Record<Provider, string> = {
  forum: "forum challenge service not configured (needs DISCOURSE_API_KEY, GATSBY_DISCOURSE_API, GATSBY_DISCOURSE_USER and GATSBY_DISCOURSE_CONNECT_THREAD)",
  discord:
    "Discord verification bot not configured (needs DISCORD_TOKEN and DISCORD_PROFILE_VERIFICATION_CHANNEL_ID, plus the bot joined to the guild that owns that channel)",
  push: "Push Protocol subscription is signed by your own wallet against channel 0x4BaaC83d0A68C079550142B9d792328b7C239844 and has no server-side path",
};

export const failClosedVerify: VerifyAccountFn = async ({ provider }) => {
  throw new Error(`account verification unavailable: ${PROVIDER_BLOCKER[provider]}`);
};

export type UnlinkResult = { account: Provider; unlinked: true };

export type UnlinkAccountFn = (args: {
  account: Provider;
  signal?: AbortSignal;
}) => Promise<UnlinkResult>;

export const failClosedUnlink: UnlinkAccountFn = async () => {
  throw new Error("account unlink unavailable: governance account service not configured");
};
