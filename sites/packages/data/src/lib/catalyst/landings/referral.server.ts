import fs from "node:fs";
import path from "node:path";

import {
  fetchReferrerProfile,
  referrerFromParam,
  ReferralStateSchema,
  type ReferrerVM,
  type ReferralState,
} from "./referral";
import { ETH_ADDRESS_RE } from "../format/address";

const FIXTURE = path.join(
  process.cwd(),
  "packages",
  "data",
  "src",
  "fixtures",
  "landings-invite-referral.json",
);

type Faq = { q: string; a: string };

type Fixture = {
  _source?: string;
  referrer: { name: string; ethAddress: string; hasClaimedName?: boolean };
  referral: unknown;
  faqs: Faq[];
};

let cached: { referral: ReferralState; faqs: Faq[]; referrer: ReferrerVM } | null = null;

function readFixture() {
  if (cached) return cached;
  const raw = JSON.parse(fs.readFileSync(FIXTURE, "utf8")) as Fixture;
  const ladder = ReferralStateSchema.parse(raw.referral);
  cached = {
    referral: {
      ...ladder,
      invitedUsers: 0,
      invitedUsersAccepted: 0,
      currentTier: 0,
      nextTier: ladder.tiers[0]?.tier ?? 1,
    },
    faqs: raw.faqs ?? [],
    referrer: {
      name: raw.referrer.name,
      address: raw.referrer.ethAddress?.toLowerCase() ?? null,
      hasClaimedName: Boolean(raw.referrer.hasClaimedName),
      resolved: false,
    },
  };
  return cached;
}

export type ReferralLoad = {
  referrer: ReferrerVM;
  referral: ReferralState;
  faqs: Faq[];
  referrerParam: string;
  isFixture: boolean;
};

export async function loadInviteReferral(
  referrerParam: string,
  signal?: AbortSignal,
): Promise<ReferralLoad> {
  const fx = readFixture();
  const param = referrerParam.trim();

  let referrer: ReferrerVM;
  if (param && ETH_ADDRESS_RE.test(param)) {
    try {
      const resolved = await fetchReferrerProfile(param, { signal });
      referrer = resolved ?? referrerFromParam(param);
    } catch {
      referrer = referrerFromParam(param);
    }
  } else if (param) {
    referrer = referrerFromParam(param);
  } else {
    referrer = referrerFromParam("");
  }

  return {
    referrer,
    referral: fx.referral,
    faqs: fx.faqs,
    referrerParam: param,
    isFixture: true,
  };
}
