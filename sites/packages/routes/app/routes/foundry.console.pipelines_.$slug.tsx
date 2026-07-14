import { useEffect, useRef } from "react";

import FdPipelinePage from "@ui/foundry/pages/FdPipelinePage";

import "@ui/foundry/components/fdmarkdown.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/pages/fdpipeline.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { sidBadge } from "@data/lib/foundry/db.server";
import {
  readPipeline,
  type PipelineDetail,
} from "@data/lib/foundry/pipeline-files.server";

import type { Route } from "./+types/foundry.console.pipelines_.$slug";

export function meta({ loaderData, params }: Route.MetaArgs) {
  const title = loaderData?.detail?.title;
  return [{ title: `${title || params.slug} — The Foundry` }];
}

function toDetailVM(detail: PipelineDetail) {
  return {
    slug: detail.slug,
    title: detail.title,
    kind: detail.kind,
    created: detail.created,
    steps: detail.steps.map((step) => ({
      id: step.id,
      status: step.status,
      artifact: step.artifact,
      problems: step.problems,
      updated: step.updated,
      content: step.content,
    })),
  };
}

export async function loader({ params, request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);
  const detail = await readPipeline(params.slug);
  if (!detail) throw new Response(null, { status: 404 });
  return wrap({ badge: sidBadge(sid), detail: toDetailVM(detail) });
}

export default function FoundryPipeline({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const ctx = { sid: d.badge };

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track("fd_pipeline_viewed", { slug: d.detail.slug }, ctx);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge]);

  return (
    <FdPipelinePage
      slug={d.detail.slug}
      title={d.detail.title}
      kind={d.detail.kind}
      created={d.detail.created}
      steps={d.detail.steps}
    />
  );
}
