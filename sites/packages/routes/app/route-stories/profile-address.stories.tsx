import ProfileRoute from "../routes/profile_.$address";
import { emptyProfile } from "@data/lib/catalyst/overlay/profile";
import profileLoader from "@data/fixtures/route-profile-loader.json";
import { routeStory } from "./lib";

const base = profileLoader;

const MEMBER_TABS = ["overview", "creations", "communities", "places", "photos"];
const OWN_TABS = ["overview", "assets", "communities", "places", "photos", "referral-rewards"];

function profileStubLoader({ request }: { request: Request }) {
  const url = new URL(request.url);
  const own = url.searchParams.get("own") === "1";
  const raw = url.searchParams.get("tab")?.trim() ?? "";
  const allowed = own ? OWN_TABS : MEMBER_TABS;
  const tab = allowed.includes(raw) ? raw : "overview";
  return { ...base, own, tab };
}

const profileUrl = `/profile/${encodeURIComponent(base.address)}`;

export default {
  title: "Routes/Profile",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const Overview = {
  render: routeStory({
    Component: ProfileRoute,
    path: "/profile/:address",
    url: profileUrl,
    loaderData: base,
    loader: profileStubLoader,
  }),
};

export const CreationsTab = {
  render: routeStory({
    Component: ProfileRoute,
    path: "/profile/:address",
    url: `${profileUrl}?tab=creations`,
    loaderData: { ...base, tab: "creations" },
    loader: profileStubLoader,
  }),
};

const ZERO = "0x0000000000000000000000000000000000000001";

export const UnknownAddress = {
  render: routeStory({
    Component: ProfileRoute,
    path: "/profile/:address",
    url: `/profile/${ZERO}`,
    loaderData: {
      ...base,
      address: ZERO,
      source: "fallback",
      profileFallback: true,
      hasClaimedName: false,
      profile: emptyProfile(ZERO),
      creations: { wearables: [], emotes: [] },
    },
  }),
};
