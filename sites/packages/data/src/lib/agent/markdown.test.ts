import { createElement } from "react";
import { describe, expect, it } from "vitest";

import { blogPostCards, type BlogPost } from "@core/lib/content/blog";
import type { Place } from "../catalyst/schema";
import {
  blogIndexToMarkdown,
  blogPostToMarkdown,
  assetDetailToMarkdown,
  collectionToMarkdown,
  communityToMarkdown,
  discoverToMarkdown,
  estimateTokens,
  eventDetailToMarkdown,
  governanceLandingToMarkdown,
  governanceProjectsToMarkdown,
  governanceProposalsToMarkdown,
  homeToMarkdown,
  legalDocToMarkdown,
  markdownResponse,
  placeToMarkdown,
  placesIndexToMarkdown,
  profileToMarkdown,
  projectDetailToMarkdown,
  proposalDetailToMarkdown,
  reactNodeToMarkdown,
  transparencyToMarkdown,
  wantsMarkdown,
  whatsOnToMarkdown,
  type AgentLegalDoc,
} from "./markdown";

const NO_HTML = /<\/?[a-z][^>]*>/i;

function req(url: string, headers: Record<string, string> = {}): Request {
  return new Request(url, { headers });
}

describe("wantsMarkdown", () => {
  it("is true when Accept explicitly includes text/markdown", () => {
    expect(wantsMarkdown(req("https://x.io/blog", { accept: "text/markdown" }))).toBe(true);
  });

  it("is true when text/markdown appears among other accepted types", () => {
    expect(
      wantsMarkdown(req("https://x.io/blog", { accept: "text/html, text/markdown;q=0.9" })),
    ).toBe(true);
  });

  it("is false for a normal browser Accept header (text/html, no markdown)", () => {
    expect(
      wantsMarkdown(
        req("https://x.io/blog", {
          accept: "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8",
        }),
      ),
    ).toBe(false);
  });

  it("is false for application/json and for a missing Accept header", () => {
    expect(wantsMarkdown(req("https://x.io/blog", { accept: "application/json" }))).toBe(false);
    expect(wantsMarkdown(req("https://x.io/blog"))).toBe(false);
  });

  it("is true when ?format=md is present (case-insensitive)", () => {
    expect(wantsMarkdown(req("https://x.io/blog?format=md"))).toBe(true);
    expect(wantsMarkdown(req("https://x.io/blog?format=MD&x=1"))).toBe(true);
  });

  it("is false for other format values", () => {
    expect(wantsMarkdown(req("https://x.io/blog?format=html"))).toBe(false);
  });

  it("is true for a trailing .md path", () => {
    expect(wantsMarkdown(req("https://x.io/blog/post.md"))).toBe(true);
  });
});

describe("estimateTokens", () => {
  it("is ceil(length / 4)", () => {
    expect(estimateTokens("")).toBe(0);
    expect(estimateTokens("abcd")).toBe(1);
    expect(estimateTokens("abcde")).toBe(2);
    expect(estimateTokens("a".repeat(400))).toBe(100);
  });
});

describe("markdownResponse", () => {
  it("sets content-type, token count, and Vary headers", async () => {
    const md = "# Hello\n\nWorld";
    const res = markdownResponse(md);
    expect(res.status).toBe(200);
    expect(res.headers.get("Content-Type")).toBe("text/markdown; charset=utf-8");
    expect(res.headers.get("x-markdown-tokens")).toBe(String(estimateTokens(md)));
    expect(res.headers.get("Vary")).toBe("Accept");
    expect(res.headers.get("Cache-Control")).toBeNull();
    expect(await res.text()).toBe(md);
  });

  it("honors status and cacheControl options", () => {
    const res = markdownResponse("# x", { status: 404, cacheControl: "public, max-age=60" });
    expect(res.status).toBe(404);
    expect(res.headers.get("Cache-Control")).toBe("public, max-age=60");
  });
});

describe("reactNodeToMarkdown", () => {
  it("passes strings and numbers through", () => {
    expect(reactNodeToMarkdown("hello")).toBe("hello");
    expect(reactNodeToMarkdown(42)).toBe("42");
  });

  it("drops null / undefined / boolean", () => {
    expect(reactNodeToMarkdown(null)).toBe("");
    expect(reactNodeToMarkdown(undefined)).toBe("");
    expect(reactNodeToMarkdown(false)).toBe("");
  });

  it("renders anchors as markdown links and strong as bold", () => {
    expect(reactNodeToMarkdown(createElement("a", { href: "https://x.io" }, "link"))).toBe(
      "[link](https://x.io)",
    );
    expect(reactNodeToMarkdown(createElement("strong", null, "bold"))).toBe("**bold**");
  });

  it("flattens arrays and nested children", () => {
    expect(
      reactNodeToMarkdown(["Visit ", createElement("a", { href: "u" }, "here"), "."]),
    ).toBe("Visit [here](u).");
  });
});

describe("blogPostToMarkdown", () => {
  const post: BlogPost = {
    id: "test-post",
    slug: "test-post",
    title: "Test Post Title",
    description: "A test description of the post.",
    publishedDate: "JAN 1, 2026",
    image: { url: "", width: 1200, height: 600 },
    category: {
      id: "announcements",
      slug: "announcements",
      title: "Announcements",
      url: "/blog?category=announcements",
    },
    author: {
      id: "test-author",
      title: "Test Author",
      slug: "test-author",
      image: { url: "" },
      url: "/blog?author=test-author",
    },
    hue: 0,
    body: [
      { type: "p", content: "An opening paragraph." },
      { type: "h2", content: "A Brand-New Desktop Client" },
      { type: "quote", content: "A pull quote." },
      { type: "h3", content: "What's New for Explorers" },
      { type: "ul", items: ["Photorealistic visuals with upgraded lighting and shadows"] },
    ],
  };
  const md = blogPostToMarkdown(post);

  it("leads with the H1 title", () => {
    expect(md.startsWith(`# ${post.title}`)).toBe(true);
  });

  it("includes description and metadata (category, date, author)", () => {
    expect(md).toContain(post.description);
    expect(md).toContain(post.category.title);
    expect(md).toContain(post.publishedDate);
    expect(md).toContain(post.author.title);
  });

  it("renders body blocks as markdown (##, ###, quote, list)", () => {
    expect(md).toContain("## A Brand-New Desktop Client");
    expect(md).toContain("### What's New for Explorers");
    expect(md).toMatch(/^> /m);
    expect(md).toContain("- Photorealistic visuals with upgraded lighting and shadows");
  });

  it("contains no HTML tags", () => {
    expect(md).not.toMatch(NO_HTML);
  });
});

describe("blogIndexToMarkdown", () => {
  const cards = blogPostCards();
  const md = blogIndexToMarkdown(cards);

  it("has the index H1 and a count line", () => {
    expect(md.startsWith("# Decentraland Blog")).toBe(true);
    expect(md).toContain(`${cards.length} posts.`);
  });

  it("lists each post as an H2 with a read-more link", () => {
    expect(md).toContain(`## ${cards[0].title}`);
    expect(md).toContain(`[Read more](/blog/${cards[0].slug})`);
  });

  it("contains no HTML tags", () => {
    expect(md).not.toMatch(NO_HTML);
  });
});

describe("legalDocToMarkdown", () => {
  const doc: AgentLegalDoc = {
    title: "Terms of Use",
    intro: "Please read carefully.",
    sections: [
      {
        id: "acceptance",
        heading: "1. Acceptance of Terms",
        body: [
          "By using the service you agree to these terms.",
          { type: "h3", id: "scope", content: "1.1 Scope" },
          {
            type: "p",
            content: ["For more info visit ", createElement("a", { href: "https://dao.decentraland.org" }, "the DAO"), "."],
          },
          { type: "ul", items: ["First point", "Second point"] },
        ],
      },
    ],
  };
  const md = legalDocToMarkdown(doc);

  it("leads with the H1 title and includes the intro", () => {
    expect(md.startsWith("# Terms of Use")).toBe(true);
    expect(md).toContain("Please read carefully.");
  });

  it("renders sections (##), subheadings (###) and lists (-)", () => {
    expect(md).toContain("## 1. Acceptance of Terms");
    expect(md).toContain("### 1.1 Scope");
    expect(md).toContain("- First point");
  });

  it("flattens embedded anchors into markdown links (no HTML)", () => {
    expect(md).toContain("[the DAO](https://dao.decentraland.org)");
    expect(md).not.toMatch(NO_HTML);
  });
});

function makePlace(overrides: Partial<Place>): Place {
  return {
    id: "",
    title: null,
    description: null,
    image: null,
    owner: null,
    positions: [],
    base_position: "0,0",
    updated_at: null,
    created_at: null,
    contact_name: null,
    categories: [],
    highlighted: false,
    highlighted_image: null,
    user_count: null,
    user_visits: 0,
    favorites: 0,
    likes: 0,
    like_rate: null,
    world: false,
    world_name: null,
    ...overrides,
  };
}

describe("placeToMarkdown", () => {
  const place = makePlace({
    id: "abc-123",
    title: "Neon Arcade",
    description: "A retro arcade in the heart of the city.",
    base_position: "10,20",
    positions: ["10,20", "11,20"],
    contact_name: "Pixel Studio",
    categories: ["game", "social"],
    user_count: 42,
    likes: 100,
    favorites: 7,
    like_rate: 0.83,
    world: false,
  });
  const md = placeToMarkdown(place);

  it("leads with the H1 title and includes the description", () => {
    expect(md.startsWith("# Neon Arcade")).toBe(true);
    expect(md).toContain("A retro arcade in the heart of the city.");
  });

  it("lists key metadata as bullets", () => {
    expect(md).toContain("- Coordinates: 10,20");
    expect(md).toContain("- Parcels: 2");
    expect(md).toContain("- Creator: Pixel Studio");
    expect(md).toContain("- Visitors: 42");
    expect(md).toContain("- Categories: game, social");
    expect(md).toContain("- Approval: 83%");
  });

  it("contains no HTML tags", () => {
    expect(md).not.toMatch(NO_HTML);
  });
});

describe("placesIndexToMarkdown", () => {
  const place = makePlace({
    id: "abc-123",
    title: "Neon Arcade",
    description: "A retro arcade.",
    base_position: "10,20",
    user_count: 42,
    likes: 100,
    world: false,
  });
  const md = placesIndexToMarkdown([place]);

  it("has the index H1 and a singular count line", () => {
    expect(md.startsWith("# Decentraland Places")).toBe(true);
    expect(md).toContain("1 place.");
  });

  it("lists each place as an H2 with a view link", () => {
    expect(md).toContain("## Neon Arcade");
    expect(md).toContain("[View](/places/abc-123)");
  });

  it("contains no HTML tags", () => {
    expect(md).not.toMatch(NO_HTML);
  });
});

describe("homeToMarkdown", () => {
  it("renders the landing story (headline, beats, CTA); no HTML", () => {
    const story = {
      id: "creator", audience: "creators", kind: "creator", voice: "x",
      headline: "Your game. Your rules.", subhead: "Build anything.",
      beats: [{ title: "Fast", body: "Ship quickly.", cta: { label: "Open editor", href: "/create" } }],
      cta: { label: "Start", href: "/create" },
    } as unknown as Parameters<typeof homeToMarkdown>[0];
    const md = homeToMarkdown(story);
    expect(md.startsWith("# Your game. Your rules.")).toBe(true);
    expect(md).toContain("## Fast");
    expect(md).toContain("[Open editor](/create)");
    expect(md).toContain("[Start](/create)");
    expect(md).not.toMatch(NO_HTML);
  });
});

describe("governanceLandingToMarkdown", () => {
  it("lists active proposals with links + empty state; no HTML", () => {
    const md = governanceLandingToMarkdown([
      { id: "p1", title: "Fund the DAO", author: "alice", type: "Grant", votes: 12, time: "2d left", urgent: true },
    ]);
    expect(md.startsWith("# Decentraland Governance")).toBe(true);
    expect(md).toContain("[Fund the DAO](/governance/proposals/p1)");
    expect(md).toContain("Grant");
    expect(md).toContain("12 votes");
    expect(md).not.toMatch(NO_HTML);
    expect(governanceLandingToMarkdown([])).toContain("No active proposals");
  });
});

describe("governanceProposalsToMarkdown", () => {
  it("renders counts, page info, per-proposal metadata + view link", () => {
    const md = governanceProposalsToMarkdown({
      proposals: [
        { id: "p1", title: "Add POI", category: "poi", status: "active", author: "bob", hue: 1, passing: true, votes: 5, forPct: 80, againstPct: 20, comments: 3, time: "1d" },
      ] as unknown as Parameters<typeof governanceProposalsToMarkdown>[0]["proposals"],
      page: 1, pageCount: 2, totalFiltered: 40,
    });
    expect(md.startsWith("# Decentraland Proposals")).toBe(true);
    expect(md).toContain("40 proposals (page 1 of 2).");
    expect(md).toContain("## Add POI");
    expect(md).toContain("for 80%");
    expect(md).toContain("[View](/governance/proposals/p1)");
    expect(md).not.toMatch(NO_HTML);
  });
});

describe("proposalDetailToMarkdown", () => {
  it("renders title, metadata, budget, description; no HTML", () => {
    const p = {
      id: "p1", type: "grant", toneClass: "", catLabel: "Grant", catTone: "", status: "active",
      statusLabel: "Active", statusTone: "", title: "Fund a hackathon", author: "carol", authorHue: 1,
      published: "2026-01-01", start: "2026-01-02", finish: "2026-01-09", snapshot: "x",
      threshold: "1M VP", thresholdReached: true, yourVp: "0",
      budget: { size: "$10k", beneficiary: "0xabc", tier: "tier1" },
      description: "Sponsor a community hackathon.",
    } as unknown as Parameters<typeof proposalDetailToMarkdown>[0];
    const md = proposalDetailToMarkdown(p);
    expect(md.startsWith("# Fund a hackathon")).toBe(true);
    expect(md).toContain("- Status: Active");
    expect(md).toContain("- Voting: 2026-01-02 \u{2192} 2026-01-09");
    expect(md).toContain("(reached)");
    expect(md).toContain("## Description");
    expect(md).toContain("Sponsor a community hackathon.");
    expect(md).not.toMatch(NO_HTML);
  });
});

describe("discoverToMarkdown", () => {
  it("renders hero + events/hotspots/rituals rails; no HTML", () => {
    const content = {
      hero: { kicker: "", title: "Discover Decentraland", subtitle: "What's happening now",
        downloads: null, downloadsLabel: "", cta: { desktop: {}, epic: {} }, platforms: [] },
      rails: [
        { id: "e", kind: "events", title: "Events", viewAll: null,
          items: [{ id: "e1", category: "Music", title: "DJ Set", when: "Tonight", live: true, image: null }] },
        { id: "h", kind: "hotspots", title: "Hotspots", viewAll: null,
          items: [{ id: "h1", title: "Central Plaza", online: 42, image: null, href: "/places/x" }] },
        { id: "r", kind: "rituals", title: "Rituals", viewAll: null,
          items: [{ id: "r1", title: "Friday Trivia", day: "Fridays", image: null, href: "/places/y" }] },
      ],
      comeHangOut: { title: "", downloads: null, downloadsLabel: "" },
    } as unknown as Parameters<typeof discoverToMarkdown>[0];
    const md = discoverToMarkdown(content);
    expect(md.startsWith("# Discover Decentraland")).toBe(true);
    expect(md).toContain("What's happening now");
    expect(md).toContain("## Events");
    expect(md).toContain("DJ Set");
    expect(md).toContain("Music");
    expect(md).toContain("LIVE");
    expect(md).toContain("## Hotspots");
    expect(md).toContain("[Central Plaza](/places/x)");
    expect(md).toContain("42 online");
    expect(md).toContain("## Rituals");
    expect(md).toContain("[Friday Trivia](/places/y)");
    expect(md).not.toMatch(NO_HTML);
  });
});

describe("collectionToMarkdown", () => {
  it("renders header, stats, and items; no HTML", () => {
    const md = collectionToMarkdown({
      header: { name: "Neon Threads", isOnSale: true },
      stats: { floor: "120", creator: "artlab", creatorShort: "artlab", itemCount: 2, network: "polygon" },
      items: [
        { id: "i1", name: "Neon Jacket", category: "upper_body", sub: "", rarity: "rare", available: 5, price: "120", image: null },
        { id: "i2", name: "Neon Boots", category: "feet", sub: "", rarity: "epic", available: 0, price: "", image: null },
      ],
    } as unknown as Parameters<typeof collectionToMarkdown>[0]);
    expect(md.startsWith("# Neon Threads")).toBe(true);
    expect(md).toContain("- Creator: artlab");
    expect(md).toContain("- Items: 2");
    expect(md).toContain("- Floor: 120 MANA");
    expect(md).toContain("- On sale: yes");
    expect(md).toContain("## Items");
    expect(md).toContain("Neon Jacket");
    expect(md).toContain("120 MANA");
    expect(md).toContain("5 available");
    expect(md).not.toMatch(NO_HTML);
  });
});

describe("communityToMarkdown", () => {
  it("renders community info, members, and events; no HTML", () => {
    const detail = {
      source: "live",
      community: {
        id: "c1", name: "Builders Guild", description: "Where scene builders meet.",
        ownerAddress: "0xowner", ownerName: "chief", privacy: "public", membersCount: 128, role: "none",
      },
      members: [
        { memberAddress: "0xm1", name: "ada", role: "owner", hasClaimedName: true, profilePictureUrl: "" },
        { memberAddress: "0xm2", name: "", role: "member", hasClaimedName: false, profilePictureUrl: "" },
      ],
      events: [{ id: "ev1", name: "Build Jam", image: "", creatorName: "ada", timeLabel: "Sat 6pm" }],
    } as unknown as Parameters<typeof communityToMarkdown>[0];
    const md = communityToMarkdown(detail);
    expect(md.startsWith("# Builders Guild")).toBe(true);
    expect(md).toContain("Where scene builders meet.");
    expect(md).toContain("- Owner: chief");
    expect(md).toContain("- Privacy: public");
    expect(md).toContain("- Members: 128");
    expect(md).toContain("## Members");
    expect(md).toContain("- ada (owner)");
    expect(md).toContain("- 0xm2 (member)");
    expect(md).toContain("## Events");
    expect(md).toContain("Build Jam");
    expect(md).not.toMatch(NO_HTML);
  });
});

describe("profileToMarkdown", () => {
  it("renders name, bio, facts, info fields and links; no HTML", () => {
    const p = {
      address: "0xabc0000000000000000000000000000000000001",
      name: "pixelmancer", hasClaimedName: true, nameColor: "#fff", mutualCount: 3,
      bio: "Builder of odd little scenes.", accountUrl: "/account",
      info: [
        { key: "country", label: "Country", value: "Argentina", icon: "globe" },
        { key: "language", label: "Language", value: "", icon: "translate" },
      ],
      links: [{ title: "Portfolio", url: "https://example.com" }],
      equipped: [],
    } as unknown as Parameters<typeof profileToMarkdown>[0];
    const md = profileToMarkdown(p);
    expect(md.startsWith("# pixelmancer")).toBe(true);
    expect(md).toContain("Builder of odd little scenes.");
    expect(md).toContain("- Claimed name: yes");
    expect(md).toContain("- Mutual friends: 3");
    expect(md).toContain("## Info");
    expect(md).toContain("- Country: Argentina");
    expect(md).not.toContain("Language:");
    expect(md).toContain("## Links");
    expect(md).toContain("[Portfolio](https://example.com)");
    expect(md).not.toMatch(NO_HTML);
  });

  it("renders an explicit empty state for an address with no public profile", () => {
    const p = {
      address: "0xdef", name: "0xdef", hasClaimedName: false, nameColor: "#fff",
      mutualCount: 0, bio: "", accountUrl: "", info: [], links: [], equipped: [],
    } as unknown as Parameters<typeof profileToMarkdown>[0];
    const md = profileToMarkdown(p);
    expect(md.startsWith("# Decentraland Profile")).toBe(true);
    expect(md).toContain("No public Decentraland profile has been set for this address.");
    expect(md).toContain("- Address: 0xdef");
    expect(md).toContain("- Claimed name: no");
    expect(md).not.toContain("## Info");
  });
});

describe("assetDetailToMarkdown", () => {
  it("renders item facts, price, and listings; no HTML", () => {
    const nft = {
      name: "Cosmic Hat", image: undefined, issuedId: 0, category: "hat",
      rarity: "epic", bodyShape: "Unisex", isSmart: false, network: "polygon",
      description: "A shimmering cosmic hat.",
      owner: { address: "0xabc", name: "alice" },
      collection: { name: "Cosmics", address: "0xcol" },
      order: { price: "1,250", issuedId: 3, expiresLabel: "Aug 1, 2026" },
    } as unknown as Parameters<typeof assetDetailToMarkdown>[0];
    const listings = [
      { owner: "bob", name: "Cosmic Hat", published: "", expires: "Aug 1, 2026", issued: 3, price: "1,250", listed: true },
    ] as unknown as Parameters<typeof assetDetailToMarkdown>[1];
    const md = assetDetailToMarkdown(nft, listings);
    expect(md.startsWith("# Cosmic Hat")).toBe(true);
    expect(md).toContain("A shimmering cosmic hat.");
    expect(md).toContain("- Rarity: epic");
    expect(md).toContain("- Network: polygon");
    expect(md).toContain("- Collection: Cosmics (0xcol)");
    expect(md).toContain("- Owner: alice");
    expect(md).toContain("- Price: 1,250 MANA \u{2014} Aug 1, 2026");
    expect(md).toContain("## Listings");
    expect(md).toContain("1,250 MANA");
    expect(md).toContain("issued #3");
    expect(md).not.toMatch(NO_HTML);
  });

  it("shows 'not listed' when there is no order", () => {
    const nft = {
      name: "Plain Tee", category: "upper_body", rarity: "common", bodyShape: "Unisex",
      isSmart: false, network: "ethereum", description: "",
      owner: { address: "0xdef", name: "" }, collection: { name: "", address: "0xc2" }, order: null,
    } as unknown as Parameters<typeof assetDetailToMarkdown>[0];
    const md = assetDetailToMarkdown(nft, []);
    expect(md).toContain("- Price: not listed");
    expect(md).toContain("- Owner: 0xdef");
    expect(md).not.toContain("## Listings");
  });
});

describe("whatsOnToMarkdown", () => {
  it("renders live + upcoming events with links, coords, counts, filter note; no HTML", () => {
    const live = [
      { id: "e1", name: "DJ Night", live: true, x: 10, y: -4, user_name: "dj", total_attendees: 42 },
    ] as unknown as Parameters<typeof whatsOnToMarkdown>[0]["live"];
    const upcoming = [
      {
        id: "e2", name: "Art Expo", start_at: "2026-07-10T18:00:00Z",
        x: 0, y: 0, scene_name: "Gallery", recurrent: true, recurrent_frequency: "weekly",
      },
    ] as unknown as Parameters<typeof whatsOnToMarkdown>[0]["upcoming"];
    const md = whatsOnToMarkdown({ live, upcoming, filter: "week" });
    expect(md.startsWith("# What's On in Decentraland")).toBe(true);
    expect(md).toContain("1 live now, 1 upcoming.");
    expect(md).toContain("(filter: week)");
    expect(md).toContain("## Live now");
    expect(md).toContain("[DJ Night](/whats-on/e1)");
    expect(md).toContain("LIVE");
    expect(md).toContain("42 attending");
    expect(md).toContain("## Upcoming");
    expect(md).toContain("[Art Expo](/whats-on/e2)");
    expect(md).toContain("(0,0)");
    expect(md).toContain("weekly");
    expect(md).not.toMatch(NO_HTML);
  });

  it("shows an empty state when there are no events", () => {
    const md = whatsOnToMarkdown({ live: [], upcoming: [] });
    expect(md).toContain("0 live now, 0 upcoming.");
    expect(md).toContain("No events right now");
  });
});

describe("eventDetailToMarkdown", () => {
  it("renders title, description, metadata bullets, jump link; no HTML", () => {
    const e = {
      id: "e2", name: "Art Expo", description: "A curated show.",
      start_at: "2026-07-10T18:00:00Z", x: 12, y: 34, user_name: "curator",
      total_attendees: 7, recurrent: true, recurrent_frequency: "weekly",
      url: "https://decentraland.org/jump/x",
    } as unknown as Parameters<typeof eventDetailToMarkdown>[0];
    const md = eventDetailToMarkdown(e);
    expect(md.startsWith("# Art Expo")).toBe(true);
    expect(md).toContain("A curated show.");
    expect(md).toContain("- Host: curator");
    expect(md).toContain("- Location: (12,34)");
    expect(md).toContain("- Recurring: weekly");
    expect(md).toContain("- Attendees: 7");
    expect(md).toContain("[Jump in](https://decentraland.org/jump/x)");
    expect(md).not.toMatch(NO_HTML);
  });

  it("builds a jump URL from coordinates when url is absent", () => {
    const e = {
      id: "e3", name: "Meetup", x: 5, y: 6,
    } as unknown as Parameters<typeof eventDetailToMarkdown>[0];
    const md = eventDetailToMarkdown(e);
    expect(md).toContain("catalyst.example.com/play/?position=");
    expect(md).toContain("5%2C6");
  });
});

describe("governanceProjectsToMarkdown", () => {
  it("renders funding summary, filter note, per-project links + metadata; no HTML", () => {
    const projects = [
      {
        id: "pr1", title: "Build a Bridge", type: "grant", category: "platform",
        status: "in_progress", author: "alice", size: 50000, token: "USD",
        vestedPct: 40, releasedPct: 25,
      },
    ] as unknown as Parameters<typeof governanceProjectsToMarkdown>[0]["projects"];
    const stats = {
      count: 1, grantsCount: 1, bidsCount: 0, ongoingCount: 1, finishedCount: 0,
      grantFunding: 50000, bidFunding: 0, totalFunding: 50000,
    } as unknown as Parameters<typeof governanceProjectsToMarkdown>[0]["stats"];
    const md = governanceProjectsToMarkdown({ projects, stats, total: 3, category: "platform" });
    expect(md.startsWith("# Decentraland DAO Projects")).toBe(true);
    expect(md).toContain("1 shown");
    expect(md).toContain("of 3");
    expect(md).toContain("50,000 USD funding");
    expect(md).toContain("(category: platform)");
    expect(md).toContain("[Build a Bridge](/governance/projects/pr1)");
    expect(md).toContain("vested 40%");
    expect(md).not.toMatch(NO_HTML);
  });
});

describe("projectDetailToMarkdown", () => {
  it("renders about, facts, milestones, team, updates, links; no HTML", () => {
    const p = {
      id: "pr1", proposal_id: "pp1", title: "Build a Bridge", status: "in_progress",
      ongoingDays: 42, about: "A cross-chain bridge.", type: "grant", author: "alice",
      authorLabel: "Alice", authorHue: 1,
      links: [{ id: "l1", label: "Repo", url: "https://github.com/x" }],
      personnel: [{ id: "m1", name: "Bob", address: null, role: "Lead", about: "" }],
      milestones: [{ id: "ms1", date: "2026-02-01", title: "MVP", description: "First cut" }],
      funding: { enactedLabel: "", endLabel: "", total: "$50,000 USD", token: "USD",
        vestedAmount: "", vestedPct: 40, releasedAmount: "", releasedPct: 25 },
      vestings: [],
      updates: [{ id: "u1", status: "done", health: null, introduction: "Shipped MVP", created_at: "2026-03-01", index: 1 }],
      activity: [],
    } as unknown as Parameters<typeof projectDetailToMarkdown>[0];
    const md = projectDetailToMarkdown(p);
    expect(md.startsWith("# Build a Bridge")).toBe(true);
    expect(md).toContain("A cross-chain bridge.");
    expect(md).toContain("- Status: in_progress");
    expect(md).toContain("- Author: Alice");
    expect(md).toContain("## Milestones");
    expect(md).toContain("MVP");
    expect(md).toContain("## Team");
    expect(md).toContain("Bob \u{2014} Lead");
    expect(md).toContain("## Updates");
    expect(md).toContain("#1");
    expect(md).toContain("## Links");
    expect(md).toContain("[Repo](https://github.com/x)");
    expect(md).not.toMatch(NO_HTML);
  });
});

describe("transparencyToMarkdown", () => {
  it("renders committees with descriptions and members; no HTML", () => {
    const t = {
      source: "live",
      committees: [
        { name: "DAO Committee", description: "Holds the keys.",
          members: [{ name: "Carol", address: "0xabc", addressShort: "0xab\u{2026}c", hue: 1 }] },
      ],
    } as unknown as Parameters<typeof transparencyToMarkdown>[0];
    const md = transparencyToMarkdown(t);
    expect(md.startsWith("# Decentraland DAO Transparency")).toBe(true);
    expect(md).toContain("## DAO Committee");
    expect(md).toContain("Holds the keys.");
    expect(md).toContain("Carol (0xab\u{2026}c)");
    expect(md).not.toMatch(NO_HTML);
  });
});

