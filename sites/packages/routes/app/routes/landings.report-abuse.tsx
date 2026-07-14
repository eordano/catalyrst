import path from "node:path";
import { useMemo } from "react";

import { useAuth } from "@data/lib/auth/context";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import {
  buildSubmitReport,
  parseReportFixture,
  type ReasonOption,
  type ReportFixture,
} from "@data/lib/catalyst/landings/report";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import ReportWizard from "@features/stories/landings/report-abuse/ReportWizard";

import type { Route } from "./+types/landings.report-abuse";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "landings/report-abuse";
const EXPERIMENT_KEY = "landings_report_wizard";
const FIXTURE_PATH = path.join(process.cwd(), "packages", "data", "src", "fixtures", `${STORY}.json`);

async function loadFixture(): Promise<ReportFixture | null> {
  try {
    const fs = await import("node:fs/promises");
    const raw = await fs.readFile(FIXTURE_PATH, "utf8");
    return parseReportFixture(JSON.parse(raw));
  } catch {
    return null;
  }
}

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true, requireConfirm: true },
  experimentKey: EXPERIMENT_KEY,
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const addressOverride = url.searchParams.get("address")?.trim() || null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const fixture = await loadFixture();
  const reasonOptions: ReasonOption[] = fixture?.reasonOptions ?? [];
  const playerAddress =
    addressOverride ??
    readWallet(request) ??
    fixture?.sampleReporter?.playerAddress ??
    "";

  const payload = { sid, step, playerAddress, reasonOptions, assignment };
  return wrap(payload);
}

export default function LandingsReportAbuse({ loaderData }: Route.ComponentProps) {
  const { sid, step, playerAddress, reasonOptions, assignment } =
    loaderData;

  const { identity } = useAuth();
  const submit = useMemo(() => buildSubmitReport(identity), [identity]);

  return (
    <main className="landings-report-abuse">
      <ReportWizard
        submit={submit}
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
        playerAddress={playerAddress}
        reasonOptions={reasonOptions.length ? reasonOptions : undefined}
        initialStep={step ?? undefined}
      />
    </main>
  );
}
