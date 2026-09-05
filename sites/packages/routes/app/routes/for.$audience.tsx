import LandingStory from "@ui/web/pages/LandingStory";
import SitesChrome from "@ui/web/frames/SitesChrome";

import {
  getLandingStory,
  type LandingStory as LandingStoryData,
} from "@core/lib/content/landing-stories";
import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/for.$audience";

const STORY = "landing-for";

export function meta({ loaderData }: Route.MetaArgs) {
  const s = (loaderData as LoaderData | undefined)?.story;
  const title = s ? `${s.headline} \u{2014} Decentraland` : "Decentraland";
  return [{ title }, { name: "description", content: s?.subhead ?? "" }];
}

export async function loader({ request, params }: Route.LoaderArgs) {
  const story = getLandingStory(params.audience);
  const { sid, wrap } = sidLoader(request);

  track(
    "landing_story_viewed",
    { audience: story.id, kind: story.kind, requested: params.audience ?? "" },
    { sid, story: STORY },
  );

  const payload = { sid, story };
  return wrap(payload);
}

type LoaderData = { sid: string; story: LandingStoryData };

export default function LandingForAudienceRoute({
  loaderData,
}: Route.ComponentProps) {
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
