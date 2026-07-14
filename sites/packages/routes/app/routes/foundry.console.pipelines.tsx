import { useEffect, useRef } from "react";

import FdPipelinesPage from "@ui/foundry/pages/FdPipelinesPage";

import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/pages/fdpipelines.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { sidBadge } from "@data/lib/foundry/db.server";
import {
  listPipelines,
  type PipelineSummary,
} from "@data/lib/foundry/pipeline-files.server";

import type { Route } from "./+types/foundry.console.pipelines";

export function meta() {
  return [{ title: "Pipelines — The Foundry" }];
}

function toRowVM(pipeline: PipelineSummary) {
  return {
    slug: pipeline.slug,
    title: pipeline.title,
    kind: pipeline.kind,
    created: pipeline.created,
    passed: pipeline.passed,
    total: pipeline.total,
    next: pipeline.next,
    href: `/foundry/console/pipelines/${encodeURIComponent(pipeline.slug)}`,
  };
}

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);
  const pipelines = await listPipelines();
  return wrap({ badge: sidBadge(sid), pipelines: pipelines.map(toRowVM) });
}

export default function FoundryPipelines({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const ctx = { sid: d.badge };

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track("fd_console_module_viewed", { module: "pipelines" }, ctx);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge]);

  return <FdPipelinesPage pipelines={d.pipelines} />;
}
