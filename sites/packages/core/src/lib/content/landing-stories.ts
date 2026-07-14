export type LandingBeat = {
  title: string;
  body: string;
  cta: { label: string; href: string };
};
export type LandingKind = "creator" | "user";

export type LandingStory = {
  id: string;
  audience: string;
  kind: LandingKind;
  voice: string;
  headline: string;
  subhead: string;
  beats: LandingBeat[];
  cta: { label: string; href: string };
};

export const LANDING_STORIES: LandingStory[] = [
  {
    id: "scenes",
    audience: "Scene & game creators",
    kind: "creator",
    voice: "peer-to-peer maker energy; the Roblox/UEFN builder who wants a fair deal",
    headline: "Your game. Your rules. Your people.",
    subhead:
      "Build a 3D world people show up for \u{2014} open, yours, and unkillable.",
    beats: [
      {
        title: "Press play in seconds",
        body: "Build in the browser, drop in a cube, and watch your avatar walk your scene \u{2014} no engine install, no build wait.",
        cta: { label: "Open the editor", href: "/create" },
      },
      {
        title: "It's yours, for good",
        body: "Open-source runtime, portable assets, and a stage no platform can pull out from under you.",
        cta: { label: "See how it works", href: "/create/learn" },
      },
      {
        title: "Distribution that isn't a lottery",
        body: "Launch into a dense destination where players already are \u{2014} not an empty parcel.",
        cta: { label: "Explore the map", href: "/places" },
      },
    ],
    cta: { label: "Start building \u{2014} free", href: "/create" },
  },
  {
    id: "wearables",
    audience: "Wearable & emote designers",
    kind: "creator",
    voice: "fashion / 3D-artist, brand-owner pride",
    headline: "Sell the look. Keep the label.",
    subhead:
      "Design wearables and emotes, publish in minutes, and sell them inside the world \u{2014} you keep the upside and the IP.",
    beats: [
      {
        title: "Publish, don't wait",
        body: "A fast, predictable pipeline: submit, get listed, sell \u{2014} no black-box queue.",
        cta: { label: "Start a collection", href: "/create/wearables" },
      },
      {
        title: "Paid where people already are",
        body: "Buyers pay in stable, dollar-priced Credits right inside the experience \u{2014} no wallet gymnastics.",
        cta: { label: "How Credits work", href: "/marketplace/credits" },
      },
      {
        title: "Your work keeps working",
        body: "Every item is a real asset your buyers own and can resell \u{2014} it travels, and it keeps earning.",
        cta: { label: "Browse the marketplace", href: "/shop" },
      },
    ],
    cta: { label: "Publish your first drop", href: "/create/wearables" },
  },
  {
    id: "studios",
    audience: "Studios & pro teams",
    kind: "creator",
    voice: "B2B, credible, upside-focused; a professional shop evaluating a platform",
    headline: "Ship big. Own the upside.",
    subhead:
      "Bring your studio to an open platform with real distribution, dependable monetization, and a take rate incumbents can't match.",
    beats: [
      {
        title: "The full range",
        body: "Web and no-code up to the full SDK and VS Code \u{2014} prototype fast, scale to production.",
        cta: { label: "Explore the tools", href: "/create/learn" },
      },
      {
        title: "Recurring rails, built in",
        body: "In-scene payments, ticketed events, and items \u{2014} all priced in USD.",
        cta: { label: "See Credits", href: "/marketplace/credits" },
      },
      {
        title: "A partner that pays you to win",
        body: "We back a first cohort of creators with real support and anchor launches in a destination that has an audience.",
        cta: { label: "Talk to Creator Success", href: "/support" },
      },
    ],
    cta: { label: "Talk to Creator Success", href: "/create/about" },
  },
  {
    id: "first-timers",
    audience: "First-timers & AI-assisted makers",
    kind: "creator",
    voice: "warm, zero-intimidation; the curious person who's never made a game",
    headline: "Type a world into existence.",
    subhead:
      "Describe it, drop in a few things, press play. No code, no downloads \u{2014} your first social experience, live in the browser.",
    beats: [
      {
        title: "Start from a prompt, not a blank engine",
        body: "AI-assisted creation and drag-and-drop building get you to \u{201C}it's alive\u{201D} fast.",
        cta: { label: "Make your first scene", href: "/create" },
      },
      {
        title: "Yours the moment you make it",
        body: "Free to open, change, and share \u{2014} no gatekeepers.",
        cta: { label: "Browse templates", href: "/create/templates" },
      },
      {
        title: "Bring your friends",
        body: "Send a link and hang out inside the thing you just built.",
        cta: { label: "Find a hangout", href: "/discover" },
      },
    ],
    cta: { label: "Try it in your browser", href: "/create" },
  },
  {
    id: "players",
    audience: "Players & socializers",
    kind: "user",
    voice: "nightlife energy, FOMO; the user who comes to hang out, not to \u{201C}use a metaverse\u{201D}",
    headline: "The party already started.",
    subhead:
      "Drop into live events, games, and rooms full of real people \u{2014} no download, just a link. Show up as anyone you want.",
    beats: [
      {
        title: "Something's on right now",
        body: "Parties, tournaments, openings \u{2014} not an empty map to wander.",
        cta: { label: "See what's on", href: "/whats-on" },
      },
      {
        title: "Your look is the opener",
        body: "Your avatar, your style, your expression \u{2014} the fun starts before the game does.",
        cta: { label: "Style your avatar", href: "/shop" },
      },
      {
        title: "Free to jump in",
        body: "Play from the browser; when you buy, you pay in plain dollars and actually own it.",
        cta: { label: "Play in your browser", href: "/discover" },
      },
    ],
    cta: { label: "Jump in", href: "/discover" },
  },
  {
    id: "collectors",
    audience: "Collectors & owners",
    kind: "user",
    voice: "ownership-native, principled; the holder who values property rights + backing creators",
    headline: "Not your keys? Not your world.",
    subhead:
      "Wearables, names, land, items \u{2014} when you buy here, it's yours to keep, sell, or take with you. No landlord, no reset button.",
    beats: [
      {
        title: "Ownership by design",
        body: "Open protocols mean no company can quietly change the rules or lock you out.",
        cta: { label: "Start collecting", href: "/shop" },
      },
      {
        title: "Back creators directly",
        body: "The lowest fees in the category mean more of your money reaches the people who made the thing.",
        cta: { label: "Shop creator drops", href: "/shop" },
      },
      {
        title: "No volatility roulette",
        body: "Stable, dollar-priced Credits \u{2014} collect and trade on predictable terms.",
        cta: { label: "Get Credits", href: "/marketplace/credits" },
      },
    ],
    cta: { label: "Explore the marketplace", href: "/shop" },
  },
];

export const DEFAULT_LANDING_STORY_ID = "scenes";

export const LANDING_STORY_IDS: string[] = LANDING_STORIES.map((s) => s.id);

export function getLandingStory(id?: string | null): LandingStory {
  const match = id ? LANDING_STORIES.find((s) => s.id === id) : undefined;
  if (match) return match;
  const fallback = LANDING_STORIES.find((s) => s.id === DEFAULT_LANDING_STORY_ID);
  return fallback ?? LANDING_STORIES[0];
}

const AUDIENCE_PARAM_KEYS = [
  "a",
  "audience",
  "utm_audience",
  "utm_content",
  "utm_term",
  "utm_campaign",
] as const;

const AUDIENCE_ALIASES: Record<string, string> = {
  scenes: "scenes", creator: "scenes", creators: "scenes", games: "scenes",
  game: "scenes", gamedev: "scenes", devs: "scenes", builders: "scenes",
  wearables: "wearables", wearable: "wearables", emotes: "wearables",
  fashion: "wearables", designers: "wearables",
  studios: "studios", studio: "studios", teams: "studios", team: "studios",
  pro: "studios", agency: "studios", b2b: "studios",
  "first-timers": "first-timers", firsttimers: "first-timers", beginners: "first-timers",
  ai: "first-timers", nocode: "first-timers", newbies: "first-timers",
  players: "players", player: "players", play: "players", social: "players",
  users: "players", gamers: "players",
  collectors: "collectors", collector: "collectors", collect: "collectors",
  owners: "collectors", holders: "collectors", nft: "collectors",
};

export function resolveAudienceFromParams(params: URLSearchParams): string | null {
  for (const key of AUDIENCE_PARAM_KEYS) {
    const raw = params.get(key);
    if (!raw) continue;
    const v = raw.trim().toLowerCase();
    if (AUDIENCE_ALIASES[v]) return AUDIENCE_ALIASES[v];
    if (LANDING_STORY_IDS.includes(v)) return v;
  }
  return null;
}

function hashString(s: string): number {
  let h = 5381;
  for (let i = 0; i < s.length; i++) h = ((h << 5) + h + s.charCodeAt(i)) >>> 0;
  return h;
}

export type LandingPick = {
  story: LandingStory;
  via: "utm" | "sticky" | "random";
};

export function pickLandingStory(
  params: URLSearchParams,
  opts: { seed?: string; rng?: () => number } = {},
): LandingPick {
  const byUtm = resolveAudienceFromParams(params);
  if (byUtm) return { story: getLandingStory(byUtm), via: "utm" };

  const n = LANDING_STORIES.length;
  if (opts.seed) {
    return { story: LANDING_STORIES[hashString(opts.seed) % n], via: "sticky" };
  }
  const rng = opts.rng ?? Math.random;
  const idx = Math.min(n - 1, Math.max(0, Math.floor(rng() * n)));
  return { story: LANDING_STORIES[idx], via: "random" };
}
