import { useEffect, useRef } from "react";

import FdCopilotPage from "@ui/foundry/pages/FdCopilotPage";

import "@ui/atoms/button.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdstat.css";
import "@ui/foundry/components/fdtable.css";
import "@ui/foundry/pages/fdcopilot.css";

import { data } from "react-router";

import {
  expireSidCookie,
  serializeSidCookie,
  sharedSidDomain,
} from "@core/lib/experiments/assign";
import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import {
  copilotPublicUrl,
  doorFunnel,
  loadSkillsCatalog,
  probeCopilot,
  readDoorProbeStatus,
  skillsCatalogSource,
} from "@data/lib/foundry/copilot.server";
import { llmUsageSummary } from "@data/lib/foundry/costs.server";
import { FoundryUnavailableError, getPool, sidBadge } from "@data/lib/foundry/db.server";
import {
  copilotPipelineReceipts,
  listProgramDraftedDocs,
} from "@data/lib/foundry/program.server";
import { activeRoles } from "@data/lib/foundry/roles.server";
import { LLM_REFERENCE_PRICING } from "@data/lib/foundry/types";
import type { LlmUsageSummary } from "@data/lib/foundry/types";

import type { Route } from "./+types/foundry.copilot";

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
  return [{ title: "Copilot — The Foundry" }];
}

// The skills catalog and the pipeline description are true whether or not the
// service answered, so an offline probe costs the page its usage strip and
// nothing else.
export async function loader({ request }: Route.LoaderArgs) {
  const { sid, created } = sidLoader(request);

  let usage = EMPTY;
  let unavailable = false;

  // getPool() throws synchronously when the DB is unconfigured, so it is deferred
  // into the promise chain — evaluated inside .then() means the throw feeds the
  // .catch() rather than escaping the Promise.all literal and 500-ing the route.
  const dbNull = (err: unknown): null => {
    if (err instanceof FoundryUnavailableError) return null;
    throw err;
  };
  const [status, usageResult, pipeline, programDrafted, roles, door, doorProbe] =
    await Promise.all([
      probeCopilot(),
      Promise.resolve()
        .then(() => llmUsageSummary(getPool(), { recent: 0 }))
        .catch(dbNull),
      Promise.resolve()
        .then(() => copilotPipelineReceipts(getPool()))
        .catch(dbNull),
      Promise.resolve()
        .then(() => listProgramDraftedDocs(getPool()))
        .catch(dbNull),
      Promise.resolve()
        .then(() => activeRoles(sid))
        .catch(() => null),
      Promise.resolve()
        .then(() => doorFunnel(getPool()))
        .catch(() => null),
      readDoorProbeStatus(),
    ]);

  if (usageResult === null) unavailable = true;
  else usage = usageResult;

  // copilot.catalyst.example.com's door admits this site's session via nginx auth_request,
  // but only if the browser sends the sid cookie there — so on a *.catalyst.example.com
  // host the cookie is re-issued widened to the parent domain, and the
  // host-scope twin is expired in the same response. A legacy narrow cookie is
  // indistinguishable from a wide one in the Cookie header, so this re-issue
  // is unconditional: after it the jar holds exactly one sid.
  const domain = sharedSidDomain(request);
  const headers = new Headers();
  if (domain) {
    headers.append("Set-Cookie", serializeSidCookie(sid, { domain }));
    headers.append("Set-Cookie", expireSidCookie());
  } else if (created) {
    headers.append("Set-Cookie", serializeSidCookie(sid));
  }

  return data(
    {
      badge: sidBadge(sid),
      sessionGate:
        roles !== null && (roles.includes("admin") || roles.includes("host")),
      unavailable,
      status,
      usage,
      pipeline,
      programDrafted,
      skills: loadSkillsCatalog(),
      catalogSource: skillsCatalogSource(),
      // The enter route mints a workspace session and deep-links into it; a
      // bare copilotPublicUrl() strands visitors on the UI's folder picker.
      openUrl: copilotPublicUrl() ? "/foundry/copilot/enter" : null,
      pricing: LLM_REFERENCE_PRICING,
      door,
      doorProbe,
    },
    { headers },
  );
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
      sessionGate={d.sessionGate}
      status={d.status}
      usage={d.usage}
      usageUnavailable={d.unavailable}
      pipeline={d.pipeline}
      programDrafted={d.programDrafted}
      skills={d.skills}
      catalogSource={d.catalogSource}
      openUrl={d.openUrl}
      pricing={d.pricing}
      door={d.door}
      doorProbe={d.doorProbe}
      onOpen={() => track("fd_copilot_opened", { online: d.status.online }, ctx)}
    />
  );
}
