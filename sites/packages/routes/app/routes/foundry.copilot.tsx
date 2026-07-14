import { useEffect, useRef } from "react";

import FdCopilotPage from "@ui/foundry/pages/FdCopilotPage";

import "@ui/atoms/button.css";
import "@ui/components/emptystate.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdstat.css";
import "@ui/foundry/components/fdtable.css";
import "@ui/foundry/pages/fdcopilot.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import {
  copilotPublicUrl,
  loadSkillsCatalog,
  probeCopilot,
  skillsCatalogSource,
} from "@data/lib/foundry/copilot.server";
import { llmUsageSummary } from "@data/lib/foundry/costs.server";
import { FoundryUnavailableError, getPool, sidBadge } from "@data/lib/foundry/db.server";
import { LLM_REFERENCE_PRICING } from "@data/lib/foundry/types";
import type { LlmUsageSummary } from "@data/lib/foundry/types";

import type { Route } from "./+types/foundry.copilot";

const EMPTY: LlmUsageSummary = {
  messages: 0,
  sessions: 0,
  inputTokens: 0,
  outputTokens: 0,
  costUsd: 0,
  byDay: [],
  recent: [],
};

export function meta() {
  return [{ title: "Copilot — The Foundry" }];
}

// The skills catalog and the pipeline description are true whether or not the
// service answered, so an offline probe costs the page its usage strip and
// nothing else.
export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);

  let usage = EMPTY;
  let unavailable = false;

  // getPool() throws synchronously when the DB is unconfigured, so it is deferred
  // into the promise chain — evaluated inside .then() means the throw feeds the
  // .catch() rather than escaping the Promise.all literal and 500-ing the route.
  const [status, usageResult] = await Promise.all([
    probeCopilot(),
    Promise.resolve()
      .then(() => llmUsageSummary(getPool(), { recent: 10 }))
      .catch((err: unknown) => {
        if (err instanceof FoundryUnavailableError) return null;
        throw err;
      }),
  ]);

  if (usageResult === null) unavailable = true;
  else usage = usageResult;

  return wrap({
    badge: sidBadge(sid),
    unavailable,
    status,
    usage,
    skills: loadSkillsCatalog(),
    catalogSource: skillsCatalogSource(),
    openUrl: copilotPublicUrl(),
    pricing: LLM_REFERENCE_PRICING,
  });
}

export default function FoundryCopilot({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const ctx = { sid: d.badge };

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track(
      "fd_copilot_viewed",
      {
        online: d.status.online,
        sessions_total: d.usage.sessions,
        tokens_total: d.usage.inputTokens + d.usage.outputTokens,
      },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge]);

  return (
    <FdCopilotPage
      status={d.status}
      usage={d.usage}
      usageUnavailable={d.unavailable}
      skills={d.skills}
      catalogSource={d.catalogSource}
      openUrl={d.openUrl}
      pricing={d.pricing}
      onOpen={() => track("fd_copilot_opened", { online: d.status.online }, ctx)}
      onSkillLink={(skill) => track("fd_gdd_skill_link_clicked", { skill }, ctx)}
    />
  );
}
