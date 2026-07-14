import LandingStory from "@ui/web/pages/LandingStory";
import SitesChrome from "@ui/web/frames/SitesChrome";

import {
  pickLandingStory,
  type LandingStory as LandingStoryData,
} from "@core/lib/content/landing-stories";
import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/_index";
import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";

const STORY = "home";

export const handle = { agentMarkdown: "home" } satisfies AgentMarkdownHandle;

export function meta({ loaderData }: Route.MetaArgs) {
  const s = (loaderData as LoaderData | undefined)?.story;
  const title = s ? `${s.headline} \u{2014} Decentraland` : "Decentraland";
  return [{ title }, { name: "description", content: s?.subhead ?? "" }];
}

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const { sid, wrap } = sidLoader(request);

  const { story, via } = pickLandingStory(url.searchParams, { seed: sid });

  track(
    "landing_story_viewed",
    { audience: story.id, kind: story.kind, via },
    { sid, story: STORY },
  );

  const payload = { sid, story, via };
  return wrap(payload);
}

type LoaderData = { sid: string; story: LandingStoryData; via: string };

export default function Home({ loaderData }: Route.ComponentProps) {
  const { story } = loaderData as LoaderData;
  return (
    <SitesChrome active="play" overlayNav>
      <LandingStory
        audience={story.audience}
        headline={story.headline}
        subhead={story.subhead}
        beats={story.beats}
        cta={story.cta}
      />
    </SitesChrome>
  );
}
