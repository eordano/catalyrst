import ChScenes from "@ui/creatorhub/pages/ChScenes";
import ChScenesEmptyState from "@ui/creatorhub/pages/ChScenesEmptyState";

import type { CreatorScene } from "@data/lib/catalyst/create/index.server";
import scenesLoader from "@data/fixtures/route-create-scenes.json";

const base = scenesLoader as { sid: string; creator: string; scenes: CreatorScene[] };

function sceneHue(id: string): number {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) % 360;
  return h;
}

function grad(seed: string): string {
  const h = sceneHue(seed);
  return `linear-gradient(135deg, hsl(${h} 70% 52%), hsl(${(h + 40) % 360} 60% 28%))`;
}

function toProjectCard(s: CreatorScene) {
  return {
    id: s.id,
    title: s.title,
    thumbnail: s.image ?? undefined,
    layout: { cols: Math.max(1, s.parcels), rows: 1 },
    grad: grad(s.id),
    published: true,
    hasDeployments: false,
  };
}

export default {
  title: "Routes/CreateScenes",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const MyScenes = {
  render: () => (
    <ChScenes
      state="default"
      signedIn={false}
      account=""
      name=""
      projects={base.scenes.map(toProjectCard)}
    />
  ),
};

export const EmptyState = {
  render: () => <ChScenesEmptyState signedIn={false} account="" name="" />,
};
