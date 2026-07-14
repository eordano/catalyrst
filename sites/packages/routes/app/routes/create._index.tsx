import path from "node:path";

import { useEffect, useRef, useState } from "react";
import { useNavigate, useRevalidator, useSearchParams } from "react-router";

import CreatorHubHome, {
  type ChHappening,
  type ChNetworkScene,
  type ChTemplateCard,
} from "@ui/creatorhub/pages/CreatorHubHome";
import { STARTER_TEMPLATES } from "@ui/creatorhub/pages/ChTemplates";

import { useAuth } from "@data/lib/auth/index";
import { isDesktopShell } from "@data/lib/auth/native-shell";
import { openSignIn } from "@features/components/auth/signin-store";
import { loadCreatorScenes, type CreatorScene } from "@data/lib/catalyst/create/index.server";
import { fetchMostActivePlaces } from "@data/lib/catalyst/places/index";
import { fetchEvents, type Event } from "@data/lib/catalyst/places/events";
import { blogPostCards } from "@core/lib/content/blog";
import { fetchProfile } from "@data/lib/catalyst/overlay/profile";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { resolveAssignment } from "@core/lib/experiments/assign";
import { parseStory } from "@core/lib/experiments/context";
import { sidLoader } from "@core/lib/experiments/story-loader";
import {
  CREATE_ENTRY_TARGETS,
  activeCreateExperiment,
  armOverride,
  entryFromFlags,
  webHubIfCapable,
  type CreateEntryConfig,
} from "@core/lib/experiments/create-entry";
import { supportsDirectoryPicker } from "@data/lib/fs/disk";
import { track, trackExposure } from "@core/lib/telemetry/track";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/create._index";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("Creator Hub");

const STORY: StoryId = "create/hub-to-scenes";

const NETWORK_STRIP_LIMIT = 6;
const HAPPENING_EVENTS_LIMIT = 3;
const HAPPENING_POSTS_LIMIT = 3;

function happeningDate(value: string | null): string {
  if (!value) return "";
  const parsed = new Date(value);
  if (isNaN(parsed.getTime())) return "";
  return parsed.toLocaleDateString("en-US", { weekday: "short", month: "short", day: "numeric" });
}

function eventToHappening(e: Event): ChHappening {
  return {
    id: e.id,
    kind: "event",
    title: e.name ?? "Untitled event",
    image: e.image,
    meta: e.live ? "Happening now" : happeningDate(e.next_start_at ?? e.start_at),
    href: `/whats-on/${encodeURIComponent(e.id)}`,
    live: e.live,
  };
}

async function loadNetworkStrip(signal: AbortSignal): Promise<ChNetworkScene[]> {
  const places = await fetchMostActivePlaces({ limit: NETWORK_STRIP_LIMIT }, { signal });
  return places.map((p) => ({
    id: p.id,
    title: p.title ?? "Untitled scene",
    image: p.image,
    users: p.user_count ?? 0,
    href: `/places/${encodeURIComponent(p.id)}`,
  }));
}

async function loadHappeningEvents(signal: AbortSignal): Promise<ChHappening[]> {
  for (const list of ["trending", "active"] as const) {
    try {
      const { data } = await fetchEvents({ list, limit: HAPPENING_EVENTS_LIMIT }, { signal });
      if (data.length > 0) return data.map(eventToHappening);
    } catch {
      // fall through to the next list; an empty rail is handled by the caller
    }
  }
  return [];
}

function loadHappeningPosts(): ChHappening[] {
  return blogPostCards()
    .slice(0, HAPPENING_POSTS_LIMIT)
    .map((p) => ({
      id: p.id,
      kind: "post" as const,
      title: p.title,
      hue: p.hue,
      meta: [p.category.title, happeningDate(p.publishedDate)].filter(Boolean).join(" \u{B7} "),
      href: `/blog/${encodeURIComponent(p.slug)}`,
    }));
}

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const creator = url.searchParams.get("creator")?.trim() || readWallet(request) || "";

  const { sid, wrap } = sidLoader(request);

  let scenes: CreatorScene[] = [];
  let scenesError = false;
  const [scenesResult, network, happeningEvents] = await Promise.all([
    loadCreatorScenes({ creator: creator || undefined, limit: 6 }).catch(() => null),
    loadNetworkStrip(request.signal).catch(() => [] as ChNetworkScene[]),
    loadHappeningEvents(request.signal),
  ]);
  if (scenesResult === null) {
    scenesError = true;
  } else {
    scenes = scenesResult;
  }
  const happenings = [...happeningEvents, ...loadHappeningPosts()];

  let entry: CreateEntryConfig | null = null;
  const experiment = activeCreateExperiment(
    typeof process !== "undefined" ? process.env?.CREATE_EXPERIMENT : undefined,
  );
  if (experiment) {
    try {
      const story = parseStory(
        path.join(process.cwd(), "packages", "features", "src", "stories", "create", experiment),
      );
      let assignment = await resolveAssignment(sid, story);

      const forced = armOverride(url, story.experiment.variants);
      if (forced) {
        const v = story.experiment.variants.find((x) => x.id === forced);
        if (v) {
          assignment = {
            variant: v.id,
            flags: v.flags,
            experimentKey: story.experiment.key,
          };
        }
      }

      const storyTag = `create/${experiment}`;
      trackExposure({
        sid,
        story: storyTag,
        variant: assignment.variant,
        experimentKey: assignment.experimentKey,
      });

      entry = {
        story: storyTag,
        experimentKey: assignment.experimentKey,
        variant: assignment.variant,
        entry: entryFromFlags(assignment.flags),
        webHubIfCapable: webHubIfCapable(assignment.flags),
      };
    } catch {
      entry = null;
    }
  }

  const payload = { sid, creator, scenes, scenesError, entry, network, happenings };
  return wrap(payload);
}

export default function CreateHome({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  return (
    <HubHomeView
      sid={d.sid}
      creator={d.creator}
      scenes={d.scenes}
      scenesError={d.scenesError}
      entry={d.entry}
      network={d.network}
      happenings={d.happenings}
    />
  );
}

const TEMPLATE_CARDS: ChTemplateCard[] = STARTER_TEMPLATES.map((t) => ({
  id: t.id,
  title: t.title,
  thumb: t.thumb,
  difficulty: t.difficulty_level,
  href: `/creator-hub/scene-editor?new=1&template=${encodeURIComponent(t.id)}&name=${encodeURIComponent(t.title)}&from=home`,
}));

function HubHomeView({
  sid,
  creator,
  scenes,
  scenesError,
  entry,
  network,
  happenings,
}: {
  sid: string;
  creator: string;
  scenes: CreatorScene[];
  scenesError: boolean;
  entry: CreateEntryConfig | null;
  network: ChNetworkScene[];
  happenings: ChHappening[];
}) {
  const navigate = useNavigate();
  const revalidator = useRevalidator();
  const [, setSearchParams] = useSearchParams();
  const { isConnected, address } = useAuth();

  const [name, setName] = useState("");

  useEffect(() => {
    if (!isDesktopShell()) return;
    type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
    const tauri = (window as { __TAURI__?: { core: { invoke: Invoke } } }).__TAURI__;
    if (!tauri) {
      window.location.assign("/create?mp_ipc_error=no-global-tauri");
      return;
    }
    tauri.core
      .invoke("mp_hello", { name: "create-home" })
      .then((r) => tauri.core.invoke("mp_log", { msg: `round-trip ok: ${String(r)}` }))
      .catch((e: unknown) => {
        window.location.assign(`/create?mp_ipc_error=${encodeURIComponent(String(e))}`);
      });
  }, []);

  useEffect(() => {
    if (isConnected && address && !creator) {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          next.set("creator", address);
          return next;
        },
        { replace: true, preventScrollReset: true },
      );
    }
  }, [isConnected, address, creator, setSearchParams]);

  useEffect(() => {
    if (!isConnected || !address) return;
    let cancelled = false;
    void (async () => {
      try {
        const avatar = await fetchProfile(address);
        const resolved = avatar?.name?.trim() ?? "";
        if (!cancelled && resolved) setName(resolved);
      } catch {
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [isConnected, address]);

  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;

    track("ch_home_viewed", { scene_count: scenes.length }, { sid, story: STORY });

    if (entry) {
      track(
        "create_entry_viewed",
        { arm: entry.variant },
        { sid, story: entry.story, variant: entry.variant, experimentKey: entry.experimentKey },
      );
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sid]);

  const rescoping = isConnected && !!address && !creator;

  const sceneRows = scenes.map((s) => ({
    id: s.id,
    title: s.title,
    href: s.base_position
      ? `/creator-hub/scene-editor?pointer=${encodeURIComponent(s.base_position)}&from=home`
      :
        `/creator-hub/scene-editor?new=1&name=${encodeURIComponent(s.title)}&from=home`,
  }));

  function onScenesRetry() {
    track("ch_home_card_opened", { card: "scenes_retry" }, { sid, story: STORY });
    if (typeof revalidator.revalidate === "function") {
      revalidator.revalidate();
    } else {
      navigate(0);
    }
  }

  return (
      <CreatorHubHome
        banner={entry?.entry ? <CreateEntrySurface sid={sid} entry={entry} /> : null}
        scenes={sceneRows}
        scenesError={scenesError}
        rescoping={rescoping}
        network={network}
        templates={TEMPLATE_CARDS}
        happenings={happenings}
        onCardClick={(card) => {
          track("ch_home_card_opened", { card }, { sid, story: STORY });
        }}
        onRetry={onScenesRetry}
        signedIn={isConnected}
        account={address ?? ""}
        name={name}
        onSignIn={() => {
          track("ch_home_signin_clicked", {}, { sid, story: STORY });
          openSignIn();
        }}
        onStartBuilding={() => {
          track("ch_home_start_building", {}, { sid, story: STORY });
          if (typeof window !== "undefined") {
            window.location.assign("/creator-hub/scene-editor?new=1&from=home");
          } else {
            navigate("/creator-hub/scene-editor?new=1&from=home");
          }
        }}
        onScenes={() => {
          track("ch_scenes_clicked", { from: "home", card: "scenes" }, { sid, story: STORY });
        }}
        onLearn={() => {
          track("ch_home_card_opened", { card: "learn" }, { sid, story: STORY });
        }}
      />
  );
}

function CreateEntrySurface({ sid, entry }: { sid: string; entry: CreateEntryConfig }) {
  const ctx = {
    sid,
    story: entry.story,
    variant: entry.variant,
    experimentKey: entry.experimentKey,
  };

  const [fsAccess, setFsAccess] = useState<boolean | null>(null);
  const isCapabilityArm = entry.entry === "capability-routed";
  useEffect(() => {
    if (!isCapabilityArm) return;
    const capable = entry.webHubIfCapable && supportsDirectoryPicker();
    setFsAccess(capable);
    try {
      const key = `create_capability_detected:${entry.experimentKey}`;
      if (!sessionStorage.getItem(key)) {
        sessionStorage.setItem(key, "1");
        track(
          "create_capability_detected",
          { fs_access: capable, route: capable ? "web-hub" : "download" },
          ctx,
        );
      }
    } catch {
      track(
        "create_capability_detected",
        { fs_access: capable, route: capable ? "web-hub" : "download" },
        ctx,
      );
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isCapabilityArm]);

  const onBuilder = () =>
    track("create_preview", { path: "builder", target: CREATE_ENTRY_TARGETS.builder }, ctx);
  const onWebHub = () =>
    track("create_preview", { path: "web-hub", target: CREATE_ENTRY_TARGETS.webHub }, ctx);
  const onDownload = () =>
    track("create_download_clicked", { from: entry.entry ?? "control" }, ctx);

  const downloadCta = (primary: boolean) => (
    <a
      href={CREATE_ENTRY_TARGETS.download}
      onClick={onDownload}
      style={primary ? ENTRY_CTA_PRIMARY : ENTRY_CTA_SECONDARY}
    >
      Download Creator Hub
    </a>
  );

  switch (entry.entry) {
    case "download-hub":
      return (
        <EntryPanel
          title="Get the Creator Hub"
          subtitle={"Download the desktop Creator Hub \u{2014} the full toolset for building scenes, wearables and Worlds."}
        >
          {downloadCta(true)}
        </EntryPanel>
      );
    case "builder-or-download":
      return (
        <EntryPanel
          title="Make a wearable now"
          subtitle="Make a wearable right here in your browser, or get the full desktop toolset."
        >
          <a href={CREATE_ENTRY_TARGETS.builder} onClick={onBuilder} style={ENTRY_CTA_PRIMARY}>
            Make a wearable in your browser
          </a>
          {downloadCta(false)}
        </EntryPanel>
      );
    case "hub-or-download":
      return (
        <EntryPanel
          title="Open the Creator Hub"
          subtitle="Open the Creator Hub in your browser and jump straight into a scene, or get the desktop app."
        >
          <a href={CREATE_ENTRY_TARGETS.webHub} onClick={onWebHub} style={ENTRY_CTA_PRIMARY}>
            Open Creator Hub in your browser
          </a>
          {downloadCta(false)}
        </EntryPanel>
      );
    case "capability-routed":
      return (
        <EntryPanel
          title="Start creating now"
          subtitle={
            fsAccess
              ? "Your browser supports local file access \u{2014} open the Creator Hub right here, no install needed."
              : "Get the Creator Hub and start building scenes, wearables and Worlds."
          }
        >
          {fsAccess ? (
            <a href={CREATE_ENTRY_TARGETS.webHub} onClick={onWebHub} style={ENTRY_CTA_PRIMARY}>
              Open Creator Hub in your browser
            </a>
          ) : (
            downloadCta(true)
          )}
        </EntryPanel>
      );
    default:
      return null;
  }
}

function EntryPanel({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle: string;
  children: React.ReactNode;
}) {
  return (
    <section style={ENTRY_PANEL} aria-label="Start creating">
      <div style={ENTRY_COPY}>
        <h2 style={ENTRY_TITLE}>{title}</h2>
        <p style={ENTRY_SUBTITLE}>{subtitle}</p>
      </div>
      <div style={ENTRY_ACTIONS}>{children}</div>
    </section>
  );
}

const ENTRY_PANEL: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  flexWrap: "wrap",
  gap: 16,
  margin: "0 0 20px",
  padding: "18px 22px",
  border: "1px solid var(--line)",
  borderRadius: "var(--r-card)",
  background: "color-mix(in srgb, var(--brand) 8%, var(--fill-1))",
};
const ENTRY_COPY: React.CSSProperties = { minWidth: 240, flex: "1 1 320px" };
const ENTRY_TITLE: React.CSSProperties = { margin: 0, fontSize: 18, lineHeight: 1.3 };
const ENTRY_SUBTITLE: React.CSSProperties = {
  margin: "4px 0 0",
  color: "var(--ink-7)",
  fontSize: 14,
  lineHeight: 1.5,
};
const ENTRY_ACTIONS: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  flexWrap: "wrap",
  gap: 10,
};
const ENTRY_CTA_BASE: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  borderRadius: "var(--r-control)",
  padding: "10px 18px",
  fontSize: 14,
  fontWeight: 600,
  textDecoration: "none",
  cursor: "pointer",
};
const ENTRY_CTA_PRIMARY: React.CSSProperties = {
  ...ENTRY_CTA_BASE,
  background: "var(--brand)",
  color: "var(--on-brand)",
};
const ENTRY_CTA_SECONDARY: React.CSSProperties = {
  ...ENTRY_CTA_BASE,
  border: "1px solid var(--line)",
  background: "var(--fill-2)",
  color: "var(--text)",
};
