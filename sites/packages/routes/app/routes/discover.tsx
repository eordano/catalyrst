import { useEffect, useRef } from "react";

import SitesHome from "@ui/web/pages/SitesHome";
import "@ui/web/pages/siteshome.css";

import {
  heroEventCards,
  hotspotCards,
  ritualCards,
  type HomeContent,
} from "@data/lib/catalyst/landings/home";
import { loadHome } from "@data/lib/catalyst/landings/home.server";
import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/discover";
import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";

export const handle = { agentMarkdown: "discover" } satisfies AgentMarkdownHandle;

const STORY = "discover";

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);
  const { content, live } = await loadHome();
  const payload = { sid, content, live };
  return wrap(payload);
}

type LoaderData = { sid: string; content: HomeContent; live: boolean };

export default function DiscoverRoute({ loaderData }: Route.ComponentProps) {
  const { sid, content } = loaderData as LoaderData;

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track("lp_discover_viewed", {}, { sid, story: STORY });
  }, [sid]);

  return (
    <SitesHome
      downloads={content.hero.downloads}
      events={heroEventCards(content)}
      hotspots={hotspotCards(content)}
      rituals={ritualCards(content)}
    />
  );
}
