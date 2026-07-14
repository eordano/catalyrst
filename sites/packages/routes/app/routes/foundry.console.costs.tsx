import { useEffect, useRef } from "react";

import FdCostsPage from "@ui/foundry/pages/FdCostsPage";

import "@ui/atoms/button.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdstat.css";
import "@ui/foundry/components/fdtable.css";
import "@ui/foundry/pages/fdcosts.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { llmUsageSummary } from "@data/lib/foundry/costs.server";
import { FoundryUnavailableError, getPool, sidBadge } from "@data/lib/foundry/db.server";
import { LLM_REFERENCE_PRICING } from "@data/lib/foundry/types";
import type { LlmUsageSummary } from "@data/lib/foundry/types";

import type { Route } from "./+types/foundry.console.costs";

const EMPTY: LlmUsageSummary = {
  messages: 0,
  sessions: 0,
  inputTokens: 0,
  outputTokens: 0,
  costUsd: 0,
  probeMessages: 0,
  probeTokens: 0,
  probeCostUsd: 0,
  byDay: [],
  recent: [],
};

export function meta() {
  return [{ title: "Copilot costs — The Foundry" }];
}

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);

  let usage = EMPTY;
  let unavailable = false;

  try {
    usage = await llmUsageSummary(getPool(), { recent: 50 });
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    unavailable = true;
  }

  return wrap({
    badge: sidBadge(sid),
    unavailable,
    usage,
    pricing: LLM_REFERENCE_PRICING,
  });
}

export default function FoundryCosts({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const ctx = { sid: d.badge };

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track("fd_console_module_viewed", { module: "costs" }, ctx);
    track(
      "fd_costs_viewed",
      {
        messages: d.usage.messages,
        tokens_total: d.usage.inputTokens + d.usage.outputTokens,
      },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge]);

  return (
    <FdCostsPage usage={d.usage} pricing={d.pricing} unavailable={d.unavailable} />
  );
}
