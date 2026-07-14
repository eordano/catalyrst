import { z } from "zod";

import type { Event } from "../places/events";
import { formatEventTime } from "../places/events";
import type { Place } from "../schema";

const LinkSchema = z.object({
  label: z.string(),
  href: z.string(),
});

const CtaTargetSchema = z.object({
  label: z.string(),
  href: z.string(),
});

const HeroSchema = z.object({
  kicker: z.string(),
  title: z.string(),
  subtitle: z.string(),
  downloads: z.string().nullish().transform((v) => v ?? null),
  downloadsLabel: z.string(),
  cta: z.object({
    desktop: CtaTargetSchema,
    epic: CtaTargetSchema,
  }),
  platforms: z.array(
    z.object({ id: z.string(), label: z.string(), href: z.string() }),
  ),
});

const EventRailItemSchema = z.object({
  id: z.string(),
  category: z.string(),
  title: z.string(),
  when: z.string(),
  live: z.boolean(),
  image: z.string().nullish().transform((v) => v ?? null),
});

const HotspotRailItemSchema = z.object({
  id: z.string(),
  title: z.string(),
  online: z.number().nullish().transform((v) => v ?? null),
  image: z.string().nullish().transform((v) => v ?? null),
  href: z.string(),
});

const RitualRailItemSchema = z.object({
  id: z.string(),
  title: z.string(),
  day: z.string().nullish().transform((v) => v ?? null),
  image: z.string().nullish().transform((v) => v ?? null),
  href: z.string(),
});

const RailSchema = z.object({
  id: z.string(),
  kind: z.enum(["events", "hotspots", "rituals"]),
  title: z.string(),
  viewAll: LinkSchema.nullish().transform((v) => v ?? null),
  items: z.array(z.unknown()),
});

export const HomeContentSchema = z.object({
  hero: HeroSchema,
  rails: z.array(RailSchema),
  comeHangOut: z.object({
    title: z.string(),
    downloads: z.string().nullish().transform((v) => v ?? null),
    downloadsLabel: z.string(),
  }),
});

export type HomeContent = z.infer<typeof HomeContentSchema>;
export type Rail = z.infer<typeof RailSchema>;
export type EventRailItem = z.infer<typeof EventRailItemSchema>;
export type HotspotRailItem = z.infer<typeof HotspotRailItemSchema>;
export type RitualRailItem = z.infer<typeof RitualRailItemSchema>;

/**
 * Throws rather than casting. The only input is the `HOME_CONTENT` constant in
 * home.server.ts, so a failure here is a bug in this repo, not a bad upstream --
 * and `raw as HomeContent` handed the page a hero and a rail list that had just
 * been rejected, with the type still claiming they were checked.
 */
export function parseHomeContent(raw: unknown): HomeContent {
  return HomeContentSchema.parse(raw);
}

function eventCategory(e: Event): string {
  if (e.live) return "LIVE";
  if (e.trending) return "TRENDING";
  if (e.highlighted) return "FEATURED";
  return "EVENT";
}

export function eventToRailItem(e: Event): EventRailItem {
  return {
    id: e.id,
    category: eventCategory(e),
    title: e.name ?? "Untitled event",
    when: formatEventTime(e.start_at ?? e.next_start_at),
    live: e.live ?? false,
    image: e.image ?? null,
  };
}

const WEEKDAY_NAMES = [
  "Sunday",
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
];

export function eventWeekday(e: Event): number | null {
  const iso = e.next_start_at ?? e.start_at;
  if (!iso) return null;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return null;
  return d.getUTCDay();
}

export function eventToRitualItem(e: Event): RitualRailItem {
  const day = eventWeekday(e);
  return {
    id: e.id,
    title: e.name ?? "Untitled event",
    day: day == null ? null : WEEKDAY_NAMES[day] ?? null,
    image: e.image ?? null,
    href: `/whats-on/${encodeURIComponent(e.id)}`,
  };
}

export function placeToHotspotItem(p: Place): HotspotRailItem {
  return {
    id: p.id,
    title: p.title ?? "Untitled place",
    online: p.user_count ?? null,
    image: p.image ?? null,
    href: `https://catalyst.example.com/play/?position=${encodeURIComponent(p.base_position)}`,
  };
}

export function withLiveEvents(content: HomeContent, live: Event[]): HomeContent {
  if (!live.length) return content;
  const items = live.map(eventToRailItem);
  return {
    ...content,
    rails: content.rails.map((rail) =>
      rail.kind === "events" ? { ...rail, items } : rail,
    ),
  };
}

export function withHotspots(
  content: HomeContent,
  hotspots: HotspotRailItem[],
): HomeContent {
  if (!hotspots.length) return content;
  return {
    ...content,
    rails: content.rails.map((rail) =>
      rail.kind === "hotspots" ? { ...rail, items: hotspots } : rail,
    ),
  };
}

export function withRituals(
  content: HomeContent,
  rituals: RitualRailItem[],
): HomeContent {
  if (!rituals.length) return content;
  return {
    ...content,
    rails: content.rails.map((rail) =>
      rail.kind === "rituals" ? { ...rail, items: rituals } : rail,
    ),
  };
}

export function eventItems(rail: Rail): EventRailItem[] {
  return rail.items
    .map((it) => EventRailItemSchema.safeParse(it))
    .filter((r): r is { success: true; data: EventRailItem } => r.success)
    .map((r) => r.data);
}

export function hotspotItems(rail: Rail): HotspotRailItem[] {
  return rail.items
    .map((it) => HotspotRailItemSchema.safeParse(it))
    .filter((r): r is { success: true; data: HotspotRailItem } => r.success)
    .map((r) => r.data);
}

export function ritualItems(rail: Rail): RitualRailItem[] {
  return rail.items
    .map((it) => RitualRailItemSchema.safeParse(it))
    .filter((r): r is { success: true; data: RitualRailItem } => r.success)
    .map((r) => r.data);
}

export function heroEventCards(content: HomeContent) {
  const rail = content.rails.find((r) => r.kind === "events");
  if (!rail) return undefined;
  const items = eventItems(rail).slice(0, 3);
  if (!items.length) return undefined;
  return items.map((ev) => ({
    id: ev.id,
    cat: ev.category,
    title: ev.title,
    when: ev.live ? "Live now" : ev.when,
    live: ev.live,
    image: ev.image,
  }));
}

export function hotspotCards(content: HomeContent): HotspotRailItem[] | undefined {
  const rail = content.rails.find((r) => r.kind === "hotspots");
  if (!rail) return undefined;
  const items = hotspotItems(rail);
  return items.length ? items : undefined;
}

export function ritualCards(content: HomeContent): RitualRailItem[] | undefined {
  const rail = content.rails.find((r) => r.kind === "rituals");
  if (!rail) return undefined;
  const items = ritualItems(rail);
  return items.length ? items : undefined;
}
