import type { ChHappening, ChNetworkScene } from "@ui/creatorhub/pages/CreatorHubHome";
import { blogPostCards } from "@core/lib/content/blog";
import { readWallet } from "../auth/wallet-cookie";
import { loadCreatorScenes } from "../catalyst/create/index.server";
import { fetchMostActivePlaces } from "../catalyst/places/index";
import { fetchEvents, type Event } from "../catalyst/places/events";
import { fetchProfile } from "../catalyst/overlay/profile";
import { ttlMemo } from "../ttl-memo";
import { withDeadline } from "../request-deadline";
import { screenSections } from "./section.server";

function happeningDate(value: string | null): string {
  if (!value) return "";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "" : date.toLocaleDateString("en-US", {
    weekday: "short", month: "short", day: "numeric",
  });
}

function eventCard(event: Event): ChHappening {
  return {
    id: event.id, kind: "event", title: event.name ?? "Untitled event", image: event.image,
    meta: event.live ? "Happening now" : happeningDate(event.next_start_at ?? event.start_at),
    href: `/whats-on/${encodeURIComponent(event.id)}`, live: event.live,
  };
}

const network = ttlMemo({
  ttlMs: 30_000,
  load: () => withDeadline(async (signal): Promise<ChNetworkScene[]> => {
    const places = await fetchMostActivePlaces({ limit: 6 }, { signal });
    return places.map((p) => ({
      id: p.id, title: p.title ?? "Untitled scene", image: p.image,
      users: p.user_count ?? 0, href: `/places/${encodeURIComponent(p.id)}`,
    }));
  }, 3_000),
});

const events = ttlMemo({
  ttlMs: 30_000,
  load: () => withDeadline(async (signal) => {
    const results = await Promise.allSettled([
      withDeadline((child) => fetchEvents({ list: "trending", limit: 3 }, { signal: child }), 750, signal),
      withDeadline((child) => fetchEvents({ list: "active", limit: 3 }, { signal: child }), 750, signal),
    ]);
    for (const result of results) {
      if (result.status === "fulfilled" && result.value.data.length) return result.value.data.map(eventCard);
    }
    if (results.every((result) => result.status === "rejected")) throw new Error("Events unavailable");
    return [];
  }, 3_000),
});

export async function loadCreateScreen(request: Request) {
  const params = new URL(request.url).searchParams;
  const wallet = readWallet(request) ?? "";
  const creator = params.get("creator")?.trim() || wallet;
  const viewer = params.get("viewer")?.trim() || wallet || creator;
  const profileAddress = /^0x[0-9a-f]{40}$/i.test(viewer) ? viewer.toLowerCase() : "";
  const sections = screenSections(request.signal);
  const [scenes, networkResult, eventsResult, profile] = await Promise.all([
    sections.load("create_scenes", (signal) => loadCreatorScenes({ creator, limit: 6, signal })),
    sections.load("create_network", () => network(), 1_000),
    sections.load("create_events", () => events(), 1_000),
    sections.load("create_profile", (signal) => profileAddress ? fetchProfile(profileAddress, { signal }) : Promise.resolve(null)),
  ]);
  const posts: ChHappening[] = blogPostCards().slice(0, 3).map((post) => ({
    id: post.id, kind: "post", title: post.title, hue: post.hue,
    meta: [post.category.title, happeningDate(post.publishedDate)].filter(Boolean).join(" \u00b7 "),
    href: `/blog/${encodeURIComponent(post.slug)}`,
  }));
  return {
    data: {
      creator, profileAddress, profileName: profile.data?.name?.trim() ?? "",
      scenes: scenes.data ?? [], scenesError: scenes.status !== "ready",
      network: networkResult.data ?? [], happenings: [...(eventsResult.data ?? []), ...posts],
      sections: { scenes: scenes.status, network: networkResult.status, events: eventsResult.status, profile: profile.status },
    },
    serverTiming: sections.timing(),
  };
}

export function resetCreateScreenCache() {
  network.reset();
  events.reset();
}
