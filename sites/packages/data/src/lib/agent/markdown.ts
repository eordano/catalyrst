import type { BlogBlock, BlogPost, BlogPostCard } from "@core/lib/content/blog";
import type { Place } from "../catalyst/schema";
import type { LandingStory } from "@core/lib/content/landing-stories";
import type { ProposalCard, ProposalDetail } from "../catalyst/governance";
import { eventCoords, formatEventWhen, type Event } from "../catalyst/places/events";
import type { ProjectCard, ProjectStats } from "../catalyst/governance/projects";
import type { ProjectDetail } from "../catalyst/governance/project-detail";
import type { TransparencyData } from "../catalyst/governance/transparency";
import type { AssetDetail, CollectionHeader, CollectionItemRow, CollectionStats } from "../catalyst/marketplace/index";
import type { AssetListing } from "../catalyst/marketplace/orders";
import type { ProfileVM } from "../catalyst/overlay/profile";
import type { CommunityDetail } from "../catalyst/overlay/communities";
import { heroEventCards, hotspotCards, ritualCards, type HomeContent } from "../catalyst/landings/home";


export function wantsMarkdown(request: Request): boolean {
  let url: URL;
  try {
    url = new URL(request.url);
  } catch {
    return false;
  }

  const fmt = url.searchParams.get("format");
  if (fmt && fmt.toLowerCase() === "md") return true;

  if (url.pathname.toLowerCase().endsWith(".md")) return true;

  const accept = request.headers.get("accept") ?? "";
  return /(^|[\s,])text\/(x-)?markdown\b/i.test(accept);
}

export function estimateTokens(text: string): number {
  return Math.ceil(text.length / 4);
}

export type MarkdownResponseInit = {
  status?: number;
  cacheControl?: string;
  headers?: HeadersInit;
};

export function markdownResponse(md: string, opts: MarkdownResponseInit = {}): Response {
  const headers = new Headers(opts.headers);
  headers.set("Content-Type", "text/markdown; charset=utf-8");
  headers.set("x-markdown-tokens", String(estimateTokens(md)));
  const vary = headers.get("Vary");
  headers.set("Vary", vary ? `${vary}, Accept` : "Accept");
  if (opts.cacheControl) headers.set("Cache-Control", opts.cacheControl);
  return new Response(md, { status: opts.status ?? 200, headers });
}


const HEADING = ["", "# ", "## ", "### ", "#### ", "##### ", "###### "] as const;

function heading(level: number, text: string): string {
  const prefix = HEADING[Math.min(Math.max(level, 1), 6)];
  return `${prefix}${clean(text)}`;
}

function bulletList(items: string[]): string {
  return items
    .map((i) => clean(i))
    .filter((i) => i.length > 0)
    .map((i) => `- ${i}`)
    .join("\n");
}

function clean(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

function joinBlocks(blocks: Array<string | null | undefined>): string {
  return blocks
    .filter((b): b is string => typeof b === "string" && b.trim().length > 0)
    .join("\n\n")
    .trim();
}


type ElementLike = { type?: unknown; props?: { href?: unknown; children?: unknown } };

function isElementLike(node: unknown): node is ElementLike {
  return typeof node === "object" && node !== null && "props" in node;
}

export function reactNodeToMarkdown(node: unknown): string {
  if (node == null || typeof node === "boolean") return "";
  if (typeof node === "string") return node;
  if (typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(reactNodeToMarkdown).join("");

  if (isElementLike(node)) {
    const inner = reactNodeToMarkdown(node.props?.children);
    const type = node.type;
    if (type === "a" && typeof node.props?.href === "string") {
      return `[${inner}](${node.props.href})`;
    }
    if (type === "strong" || type === "b") return `**${inner}**`;
    if (type === "em" || type === "i") return `*${inner}*`;
    return inner;
  }

  return "";
}


function blogBlockToMarkdown(block: BlogBlock): string {
  switch (block.type) {
    case "h2":
      return heading(2, block.content);
    case "h3":
      return heading(3, block.content);
    case "h4":
      return heading(4, block.content);
    case "quote":
      return `> ${clean(block.content)}`;
    case "ul":
      return bulletList(block.items);
    case "p":
      return clean(block.content);
  }
}

export function blogPostToMarkdown(post: BlogPost): string {
  const meta = [post.category?.title, post.publishedDate, post.author?.title]
    .filter((v): v is string => typeof v === "string" && v.trim().length > 0)
    .join(" \u{B7} ");

  return joinBlocks([
    heading(1, post.title),
    post.description ? clean(post.description) : "",
    meta ? `_${meta}_` : "",
    ...(post.body ?? []).map(blogBlockToMarkdown),
  ]);
}

export function blogIndexToMarkdown(posts: BlogPostCard[]): string {
  const list = posts ?? [];
  const entries = list.map((p) =>
    joinBlocks([
      heading(2, p.title),
      p.description ? clean(p.description) : "",
      joinInline([p.category?.title, p.publishedDate]),
      `[Read more](/blog/${p.slug})`,
    ]),
  );

  return joinBlocks([
    heading(1, "Decentraland Blog"),
    `${list.length} ${list.length === 1 ? "post" : "posts"}.`,
    ...entries,
  ]);
}

function joinInline(parts: Array<string | null | undefined>): string {
  const joined = parts
    .filter((v): v is string => typeof v === "string" && v.trim().length > 0)
    .map(clean)
    .join(" \u{B7} ");
  return joined ? `_${joined}_` : "";
}


export type AgentLegalBlock =
  | string
  | {
      type?: string;
      id?: string;
      heading?: string;
      content?: unknown;
      items?: unknown[];
    };

export type AgentLegalSection = {
  id?: string;
  heading: string;
  body: AgentLegalBlock[];
};

export type AgentLegalDoc = {
  title: string;
  intro?: string;
  sections: AgentLegalSection[];
};

function legalBlockToMarkdown(block: AgentLegalBlock): string {
  if (typeof block === "string") return clean(block);

  const type = block.type;
  if (type === "h3") return heading(3, reactNodeToMarkdown(block.content ?? block.heading));
  if (type === "h4") return heading(4, reactNodeToMarkdown(block.content ?? block.heading));
  if (type === "ul") {
    return bulletList((block.items ?? []).map(reactNodeToMarkdown));
  }
  const text = clean(reactNodeToMarkdown(block.content ?? block.heading ?? ""));
  return text;
}

export function legalDocToMarkdown(doc: AgentLegalDoc): string {
  const sections = (doc.sections ?? []).map((section) =>
    joinBlocks([
      heading(2, section.heading),
      ...(section.body ?? []).map(legalBlockToMarkdown),
    ]),
  );

  return joinBlocks([
    heading(1, doc.title),
    doc.intro ? clean(doc.intro) : "",
    ...sections,
  ]);
}


function placeCreator(place: Place): string {
  return (place.contact_name || place.owner || "Decentraland").trim() || "Decentraland";
}

function placeApproval(place: Place): string | null {
  return typeof place.like_rate === "number"
    ? `${Math.round(place.like_rate * 100)}%`
    : null;
}

export function placeToMarkdown(place: Place): string {
  const facts: Array<[string, string | number | null | undefined]> = [
    ["Type", place.world ? `World${place.world_name ? ` (${place.world_name})` : ""}` : "Genesis City"],
    ["Coordinates", place.base_position],
    ["Parcels", (place.positions ?? []).length || null],
    ["Categories", (place.categories ?? []).length ? place.categories.join(", ") : null],
    ["Creator", placeCreator(place)],
    ["Visitors", place.user_count],
    ["Approval", placeApproval(place)],
    ["Likes", place.likes],
    ["Favorites", place.favorites],
    ["Updated", place.updated_at],
  ];

  const bullets = facts
    .filter(([, v]) => v !== null && v !== undefined && v !== "")
    .map(([k, v]) => `- ${k}: ${clean(String(v))}`)
    .join("\n");

  return joinBlocks([
    heading(1, place.title || "Untitled Place"),
    place.description ? clean(place.description) : "",
    bullets,
  ]);
}

export function placesIndexToMarkdown(list: Place[]): string {
  const places = list ?? [];
  const entries = places.map((p) => {
    const facts = [
      p.base_position ? `Coordinates: ${clean(p.base_position)}` : null,
      p.world ? "World" : null,
      typeof p.user_count === "number" ? `Visitors: ${p.user_count}` : null,
      `Likes: ${p.likes ?? 0}`,
    ]
      .filter((v): v is string => typeof v === "string" && v.length > 0)
      .join(" \u{B7} ");

    return joinBlocks([
      heading(2, p.title || "Untitled Place"),
      p.description ? clean(p.description) : "",
      facts ? `_${facts}_` : "",
      `[View](/places/${encodeURIComponent(p.id)})`,
    ]);
  });

  return joinBlocks([
    heading(1, "Decentraland Places"),
    `${places.length} ${places.length === 1 ? "place" : "places"}.`,
    ...entries,
  ]);
}


function inlineParts(parts: Array<string | null | undefined>): string {
  return parts
    .filter((v): v is string => typeof v === "string" && v.trim().length > 0)
    .map(clean)
    .join(" \u{B7} ");
}


export function homeToMarkdown(story: LandingStory): string {
  const beats = (story.beats ?? []).map((b) =>
    joinBlocks([
      heading(2, b.title),
      b.body ? clean(b.body) : "",
      b.cta ? `[${clean(b.cta.label)}](${b.cta.href})` : "",
    ]),
  );

  return joinBlocks([
    heading(1, story.headline),
    story.subhead ? clean(story.subhead) : "",
    ...beats,
    story.cta ? `[${clean(story.cta.label)}](${story.cta.href})` : "",
  ]);
}


export type AgentGovEndingSoon = {
  id: string;
  title: string;
  author?: string;
  type?: string;
  votes?: number;
  time?: string;
  urgent?: boolean;
};

export function governanceLandingToMarkdown(items: AgentGovEndingSoon[]): string {
  const list = items ?? [];
  const entries = list.map((p) => {
    const meta = inlineParts([
      p.type,
      p.author ? `by ${p.author}` : null,
      typeof p.votes === "number" ? `${p.votes} votes` : null,
      p.time,
      p.urgent ? "ending soon" : null,
    ]);
    return `- [${clean(p.title)}](/governance/proposals/${encodeURIComponent(p.id)})${
      meta ? ` \u{2014} ${meta}` : ""
    }`;
  });

  return joinBlocks([
    heading(1, "Decentraland Governance"),
    "Active proposals ending soon.",
    list.length ? entries.join("\n") : "_No active proposals._",
  ]);
}


export type AgentGovProposalsData = {
  proposals: ProposalCard[];
  page?: number;
  pageCount?: number;
  totalFiltered?: number;
};

export function governanceProposalsToMarkdown(data: AgentGovProposalsData): string {
  const list = data.proposals ?? [];
  const total = data.totalFiltered ?? list.length;
  const pageInfo =
    data.page && data.pageCount && data.pageCount > 1
      ? ` (page ${data.page} of ${data.pageCount})`
      : "";

  const entries = list.map((p) => {
    const meta = inlineParts([
      p.category,
      p.status,
      p.author ? `by ${p.author}` : null,
      typeof p.votes === "number" ? `${p.votes} votes` : null,
      typeof p.forPct === "number" ? `for ${p.forPct}%` : null,
      typeof p.againstPct === "number" ? `against ${p.againstPct}%` : null,
      p.time,
    ]);
    return joinBlocks([
      heading(2, p.title),
      meta ? `_${meta}_` : "",
      `[View](/governance/proposals/${encodeURIComponent(p.id)})`,
    ]);
  });

  return joinBlocks([
    heading(1, "Decentraland Proposals"),
    `${total} ${total === 1 ? "proposal" : "proposals"}${pageInfo}.`,
    ...entries,
  ]);
}

export function proposalDetailToMarkdown(p: ProposalDetail): string {
  const facts: Array<[string, string | number | null | undefined]> = [
    ["Type", p.catLabel || p.type],
    ["Status", p.statusLabel || p.status],
    ["Author", p.author],
    ["Published", p.published],
    ["Voting", p.start && p.finish ? `${p.start} \u{2192} ${p.finish}` : p.finish],
    ["Threshold", p.threshold ? `${p.threshold}${p.thresholdReached ? " (reached)" : ""}` : null],
    [
      "Budget",
      p.budget ? `${p.budget.size}${p.budget.beneficiary ? ` \u{2192} ${p.budget.beneficiary}` : ""}` : null,
    ],
  ];

  const bullets = facts
    .filter(([, v]) => v !== null && v !== undefined && v !== "")
    .map(([k, v]) => `- ${k}: ${clean(String(v))}`)
    .join("\n");

  return joinBlocks([
    heading(1, p.title || "Untitled Proposal"),
    bullets,
    p.description ? joinBlocks([heading(2, "Description"), clean(p.description)]) : "",
  ]);
}


export type AgentWhatsOnData = {
  live?: Event[];
  upcoming?: Event[];
  filter?: string;
  search?: string;
};

function eventLine(e: Event): string {
  const meta = inlineParts([
    e.live ? "LIVE" : formatEventWhen(e.start_at ?? e.next_start_at),
    e.recurrent ? (e.recurrent_frequency ? clean(e.recurrent_frequency) : "recurring") : null,
    eventCoords(e),
    e.user_name || e.scene_name || null,
    (e.total_attendees ?? 0) > 0 ? `${e.total_attendees} attending` : null,
  ]);
  const title = clean(e.name || "Untitled event");
  return `- [${title}](/whats-on/${encodeURIComponent(e.id)})${meta ? ` \u{2014} ${meta}` : ""}`;
}

export function whatsOnToMarkdown(data: AgentWhatsOnData): string {
  const live = data.live ?? [];
  const upcoming = data.upcoming ?? [];
  const note = inlineParts([
    data.filter ? `filter: ${data.filter}` : null,
    data.search ? `search: "${data.search}"` : null,
  ]);

  const blocks: string[] = [
    heading(1, "What's On in Decentraland"),
    `${live.length} live now, ${upcoming.length} upcoming.${note ? ` (${note})` : ""}`,
  ];
  if (live.length) blocks.push(heading(2, "Live now"), live.map(eventLine).join("\n"));
  if (upcoming.length) blocks.push(heading(2, "Upcoming"), upcoming.map(eventLine).join("\n"));
  if (!live.length && !upcoming.length) blocks.push("_No events right now. Check back soon._");

  return joinBlocks(blocks);
}

export function eventDetailToMarkdown(e: Event): string {
  const host = e.user_name || e.scene_name || "Decentraland";
  const jump =
    e.url ||
    `https://catalyst.example.com/play/?position=${encodeURIComponent(`${e.x ?? 0},${e.y ?? 0}`)}`;

  const facts: Array<[string, string | number | null | undefined]> = [
    ["When", formatEventWhen(e.start_at ?? e.next_start_at)],
    ["Host", host],
    ["Location", eventCoords(e)],
    ["Recurring", e.recurrent ? (e.recurrent_frequency || "yes") : null],
    ["Attendees", (e.total_attendees ?? 0) > 0 ? e.total_attendees : null],
    ["Live", e.live ? "yes" : null],
  ];
  const bullets = facts
    .filter(([, v]) => v !== null && v !== undefined && v !== "")
    .map(([k, v]) => `- ${k}: ${clean(String(v))}`)
    .join("\n");

  return joinBlocks([
    heading(1, e.name || "Untitled event"),
    e.description ? clean(e.description) : "",
    bullets,
    `[Jump in](${jump})`,
  ]);
}


function money(n: number | null | undefined, token = ""): string | null {
  if (typeof n !== "number" || !Number.isFinite(n)) return null;
  return `${n.toLocaleString("en-US")}${token ? ` ${token}` : ""}`;
}

export type AgentProjectsData = {
  projects?: ProjectCard[];
  stats?: ProjectStats;
  total?: number;
  category?: string;
  status?: string;
  year?: string;
  quarter?: string;
};

export function governanceProjectsToMarkdown(data: AgentProjectsData): string {
  const list = data.projects ?? [];
  const s = data.stats;
  const summary = s
    ? inlineParts([
        `${s.count} shown`,
        typeof data.total === "number" ? `of ${data.total}` : null,
        money(s.totalFunding, "USD") ? `${money(s.totalFunding, "USD")} funding` : null,
        `${s.grantsCount} grants`,
        `${s.bidsCount} bids`,
        `${s.ongoingCount} ongoing`,
        `${s.finishedCount} finished`,
      ])
    : `${list.length} projects`;
  const note = inlineParts([
    data.category ? `category: ${data.category}` : null,
    data.status ? `status: ${data.status}` : null,
    data.year ? `year: ${data.year}` : null,
    data.quarter ? `quarter: ${data.quarter}` : null,
  ]);

  const entries = list.map((p) => {
    const meta = inlineParts([
      p.type,
      p.category,
      p.status,
      p.author ? `by ${p.author}` : null,
      money(p.size, p.token),
      typeof p.vestedPct === "number" ? `vested ${p.vestedPct}%` : null,
      typeof p.releasedPct === "number" ? `released ${p.releasedPct}%` : null,
    ]);
    return `- [${clean(p.title)}](/governance/projects/${encodeURIComponent(p.id)})${
      meta ? ` \u{2014} ${meta}` : ""
    }`;
  });

  return joinBlocks([
    heading(1, "Decentraland DAO Projects"),
    `${summary}.${note ? ` (${note})` : ""}`,
    list.length ? entries.join("\n") : "_No projects match._",
  ]);
}

export function projectDetailToMarkdown(p: ProjectDetail): string {
  const facts: Array<[string, string | number | null | undefined]> = [
    ["Type", p.type],
    ["Status", p.status],
    ["Author", p.authorLabel || p.author],
    ["Ongoing", p.ongoingDays ? `${p.ongoingDays} days` : null],
    [
      "Funding",
      p.funding
        ? inlineParts([
            p.funding.total,
            typeof p.funding.vestedPct === "number" ? `vested ${p.funding.vestedPct}%` : null,
            typeof p.funding.releasedPct === "number" ? `released ${p.funding.releasedPct}%` : null,
          ])
        : null,
    ],
  ];
  const bullets = facts
    .filter(([, v]) => v !== null && v !== undefined && v !== "")
    .map(([k, v]) => `- ${k}: ${clean(String(v))}`)
    .join("\n");

  const milestones = (p.milestones ?? []).length
    ? joinBlocks([
        heading(2, "Milestones"),
        p.milestones
          .map((m) => `- ${inlineParts([m.date, clean(m.title)])}${m.description ? `: ${clean(m.description)}` : ""}`)
          .join("\n"),
      ])
    : "";
  const team = (p.personnel ?? []).length
    ? joinBlocks([
        heading(2, "Team"),
        p.personnel.map((m) => `- ${clean(m.name)}${m.role ? ` \u{2014} ${clean(m.role)}` : ""}`).join("\n"),
      ])
    : "";
  const updates = (p.updates ?? []).length
    ? joinBlocks([
        heading(2, "Updates"),
        p.updates
          .map((u) => `- ${inlineParts([`#${u.index}`, u.status, u.created_at])}${u.introduction ? ` \u{2014} ${clean(u.introduction)}` : ""}`)
          .join("\n"),
      ])
    : "";
  const links = (p.links ?? []).length
    ? joinBlocks([heading(2, "Links"), p.links.map((l) => `- [${clean(l.label || l.url)}](${l.url})`).join("\n")])
    : "";

  return joinBlocks([
    heading(1, p.title || "Untitled Project"),
    p.about ? clean(p.about) : "",
    bullets,
    milestones,
    team,
    updates,
    links,
  ]);
}

export function transparencyToMarkdown(data: TransparencyData): string {
  const committees = data.committees ?? [];
  const sections = committees.map((c) =>
    joinBlocks([
      heading(2, c.name),
      c.description ? clean(c.description) : "",
      (c.members ?? []).length
        ? c.members
            .map((m) => `- ${clean(m.name || m.addressShort)}${m.name && m.addressShort ? ` (${m.addressShort})` : ""}`)
            .join("\n")
        : "_No members listed._",
    ]),
  );

  return joinBlocks([
    heading(1, "Decentraland DAO Transparency"),
    data.source === "error"
      ? `Committee membership could not be read${data.reason ? `: ${clean(data.reason)}` : "."}`
      : "DAO committees and their members.",
    ...sections,
  ]);
}

export function assetDetailToMarkdown(nft: AssetDetail, listings: AssetListing[] = []): string {
  const facts: Array<[string, string | number | null | undefined]> = [
    ["Category", nft.category],
    ["Rarity", nft.rarity],
    ["Network", nft.network],
    ["Smart wearable", nft.isSmart ? "yes" : null],
    ["Body shape", nft.bodyShape],
    [
      "Collection",
      nft.collection?.name
        ? `${nft.collection.name}${nft.collection.address ? ` (${nft.collection.address})` : ""}`
        : nft.collection?.address,
    ],
    ["Owner", nft.owner?.name || nft.owner?.address],
    [
      "Price",
      nft.order
        ? `${nft.order.price} MANA${nft.order.expiresLabel ? ` \u{2014} ${nft.order.expiresLabel}` : ""}`
        : "not listed",
    ],
  ];
  const bullets = facts
    .filter(([, v]) => v !== null && v !== undefined && v !== "")
    .map(([k, v]) => `- ${k}: ${clean(String(v))}`)
    .join("\n");

  const listingsBlock = (listings ?? []).length
    ? joinBlocks([
        heading(2, "Listings"),
        listings
          .map((l) =>
            `- ${inlineParts([
              l.price ? `${l.price} MANA` : null,
              l.issued ? `issued #${l.issued}` : null,
              l.expires ? `expires ${l.expires}` : null,
              l.owner ? `by ${l.owner}` : null,
            ])}`,
          )
          .join("\n"),
      ])
    : "";

  return joinBlocks([
    heading(1, nft.name || "Untitled item"),
    nft.description ? clean(nft.description) : "",
    bullets,
    listingsBlock,
  ]);
}


export function profileToMarkdown(p: ProfileVM): string {
  const facts: Array<[string, string | number | null | undefined]> = [
    ["Address", p.address],
    ["Claimed name", p.hasClaimedName ? "yes" : "no"],
    ["Mutual friends", p.mutualCount > 0 ? p.mutualCount : null],
  ];
  const bullets = facts
    .filter(([, v]) => v !== null && v !== undefined && v !== "")
    .map(([k, v]) => `- ${k}: ${clean(String(v))}`)
    .join("\n");

  const info = (p.info ?? []).filter((f) => f.value);
  const links = (p.links ?? []).filter((l) => l.url);

  const empty = !p.hasClaimedName && !p.bio && info.length === 0 && links.length === 0;
  if (empty) {
    return joinBlocks([
      heading(1, "Decentraland Profile"),
      "_No public Decentraland profile has been set for this address._",
      bullets,
    ]);
  }

  const infoBlock = info.length
    ? joinBlocks([heading(2, "Info"), info.map((f) => `- ${clean(f.label)}: ${clean(f.value)}`).join("\n")])
    : "";
  const linksBlock = links.length
    ? joinBlocks([heading(2, "Links"), links.map((l) => `- [${clean(l.title || l.url)}](${l.url})`).join("\n")])
    : "";

  return joinBlocks([
    heading(1, p.name || "Decentraland Profile"),
    p.bio ? clean(p.bio) : "",
    bullets,
    infoBlock,
    linksBlock,
  ]);
}


export function discoverToMarkdown(content: HomeContent): string {
  const events = heroEventCards(content) ?? [];
  const hotspots = hotspotCards(content) ?? [];
  const rituals = ritualCards(content) ?? [];

  const blocks: string[] = [
    heading(1, content.hero?.title || "Discover Decentraland"),
    content.hero?.subtitle ? clean(content.hero.subtitle) : "",
  ];
  if (events.length)
    blocks.push(
      heading(2, "Events"),
      events.map((e) => `- ${inlineParts([clean(e.title), e.cat, e.when, e.live ? "LIVE" : null])}`).join("\n"),
    );
  if (hotspots.length)
    blocks.push(
      heading(2, "Hotspots"),
      hotspots
        .map((h) => `- ${h.href ? `[${clean(h.title)}](${h.href})` : clean(h.title)}${typeof h.online === "number" ? ` \u{2014} ${h.online} online` : ""}`)
        .join("\n"),
    );
  if (rituals.length)
    blocks.push(
      heading(2, "Rituals"),
      rituals
        .map((r) => `- ${r.href ? `[${clean(r.title)}](${r.href})` : clean(r.title)}${r.day ? ` \u{2014} ${clean(r.day)}` : ""}`)
        .join("\n"),
    );
  return joinBlocks(blocks);
}

export type AgentCollectionData = {
  header?: CollectionHeader | null;
  items?: CollectionItemRow[];
  stats?: CollectionStats | null;
};

export function collectionToMarkdown(data: AgentCollectionData): string {
  const h = data.header;
  const s = data.stats;
  const items = data.items ?? [];
  const facts: Array<[string, string | number | null | undefined]> = [
    ["Creator", s?.creator || s?.creatorShort],
    ["Items", s?.itemCount],
    ["Floor", s?.floor ? `${s.floor} MANA` : null],
    ["Network", s?.network],
    ["On sale", h?.isOnSale ? "yes" : "no"],
  ];
  const bullets = facts
    .filter(([, v]) => v !== null && v !== undefined && v !== "")
    .map(([k, v]) => `- ${k}: ${clean(String(v))}`)
    .join("\n");
  const itemsBlock = items.length
    ? joinBlocks([
        heading(2, "Items"),
        items
          .map((i) =>
            `- ${inlineParts([
              clean(i.name),
              i.rarity,
              i.category,
              i.price && i.price !== "\u{2014}" ? `${i.price} MANA` : null,
              i.available ? `${i.available} available` : null,
            ])}`,
          )
          .join("\n"),
      ])
    : "";
  return joinBlocks([heading(1, h?.name || "Collection"), bullets, itemsBlock]);
}

export function communityToMarkdown(detail: CommunityDetail): string {
  const c = detail.community;
  const facts: Array<[string, string | number | null | undefined]> = [
    ["Owner", c.ownerName || c.ownerAddress],
    ["Privacy", c.privacy],
    ["Members", c.membersCount],
  ];
  const bullets = facts
    .filter(([, v]) => v !== null && v !== undefined && v !== "")
    .map(([k, v]) => `- ${k}: ${clean(String(v))}`)
    .join("\n");

  const members = detail.members ?? [];
  const membersBlock = members.length
    ? joinBlocks([
        heading(2, "Members"),
        members
          .slice(0, 50)
          .map((m) => `- ${clean(m.name || m.memberAddress)}${m.role && m.role !== "none" ? ` (${m.role})` : ""}`)
          .join("\n"),
      ])
    : "";
  const events = detail.events ?? [];
  const eventsBlock = events.length
    ? joinBlocks([
        heading(2, "Events"),
        events
          .map((e) => `- ${inlineParts([clean(e.name), e.timeLabel, e.creatorName ? `by ${e.creatorName}` : null])}`)
          .join("\n"),
      ])
    : "";

  return joinBlocks([
    heading(1, c.name || "Community"),
    c.description ? clean(c.description) : "",
    bullets,
    membersBlock,
    eventsBlock,
  ]);
}


export type AgentMarkdownKey =
  | "blogIndex"
  | "blogPost"
  | "legalDoc"
  | "placesIndex"
  | "placeDetail"
  | "home"
  | "governanceLanding"
  | "governanceProposals"
  | "proposalDetail"
  | "whatsOn"
  | "eventDetail"
  | "governanceProjects"
  | "projectDetail"
  | "transparency"
  | "assetDetail"
  | "profileDetail"
  | "discover"
  | "collectionDetail"
  | "communityDetail";

export type AgentMarkdownHandle = { agentMarkdown: AgentMarkdownKey };
