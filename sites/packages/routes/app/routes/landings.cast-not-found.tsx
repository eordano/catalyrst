import { useSearchParams } from "react-router";

import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import CastNotFound from "@features/stories/landings/cast-not-found/CastNotFound";

import type { Route } from "./+types/landings.cast-not-found";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "landings/cast-not-found";

const DEFAULT_ASSIGNMENT: Assignment = {
  variant: "instrumented",
  flags: { instrument: true },
  experimentKey: "st_cast_not_found",
};

const FROM_VALUES = ["streamer", "watcher", "unknown"] as const;
const REASON_VALUES = ["missing", "malformed", "expired", "ended"] as const;

type CastFrom = (typeof FROM_VALUES)[number];
type CastReason = (typeof REASON_VALUES)[number];

function coerceFrom(raw: string | null): CastFrom {
  return (FROM_VALUES as readonly string[]).includes(raw ?? "")
    ? (raw as CastFrom)
    : "unknown";
}

function coerceReason(raw: string | null): CastReason {
  return (REASON_VALUES as readonly string[]).includes(raw ?? "")
    ? (raw as CastReason)
    : "missing";
}

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const from = coerceFrom(url.searchParams.get("from"));
  const reason = coerceReason(url.searchParams.get("reason"));

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    DEFAULT_ASSIGNMENT,
  );

  const trackCtx = {
    sid,
    story: STORY,
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
  };

  track("cast_not_found_shown", { from, reason }, trackCtx);

  const payload = { sid, from, reason, assignment };
  return wrap(payload);
}

export default function LandingsCastNotFound({ loaderData }: Route.ComponentProps) {
  const { sid, from, reason, assignment } = loaderData;
  const [searchParams] = useSearchParams();

  const trackCtx = {
    sid,
    story: STORY,
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
  };

  if (searchParams.get("view") === "metrics") {
    return <CastNotFoundMetrics from={from} reason={reason} />;
  }

  return (
    <main className="cast-not-found-route">
      <CastNotFound trackCtx={trackCtx} from={from} reason={reason} />
    </main>
  );
}

type MetricsProps = { from: CastFrom; reason: CastReason };

function CastNotFoundMetrics({ from, reason }: MetricsProps) {
  const events: Array<{ name: string; props: string; role: string }> = [
    {
      name: "cast_not_found_shown",
      props: "{ from, reason }",
      role: "Dead-link leak counter \u{2014} the recovery-rate denominator. Fired server-side on render.",
    },
    {
      name: "cast_not_found_go_home",
      props: "{ from, reason }",
      role: "Go Home CTA clicked (recovery to decentraland.org).",
    },
    {
      name: "cast_not_found_view_docs",
      props: "{ from, reason }",
      role: "View Documentation CTA clicked (recovery to the Cast docs).",
    },
  ];

  const cell: React.CSSProperties = {
    padding: "10px 14px",
    borderBottom: "1px solid rgba(255,255,255,0.12)",
    textAlign: "left",
    verticalAlign: "top",
  };

  return (
    <main
      style={{
        minHeight: "100dvh",
        background: "linear-gradient(135deg, #d80029 0%, #16213e 50%, #0d1117 100%)",
        color: "#fff",
        fontFamily: "Inter, Helvetica, Arial, sans-serif",
        padding: "48px 24px",
      }}
    >
      <div style={{ maxWidth: 760, margin: "0 auto" }}>
        <h1 style={{ fontSize: "2rem", margin: "0 0 8px 0" }}>
          Cast 404 {"\u{2014}"} dead-link funnel
        </h1>
        <p style={{ color: "rgba(255,255,255,0.7)", margin: "0 0 24px 0" }}>
          Telemetry shape emitted by the Cast 2.0 {"\u{201C}"}Page Not Found{"\u{201D}"} fallback. This
          arrival was attributed as{" "}
          <code>from={from}</code> {"\u{B7}"} <code>reason={reason}</code>.
        </p>

        <table
          style={{
            width: "100%",
            borderCollapse: "collapse",
            background: "rgba(0,0,0,0.25)",
            borderRadius: 8,
            overflow: "hidden",
            fontSize: "0.95rem",
          }}
        >
          <thead>
            <tr style={{ background: "rgba(0,0,0,0.35)" }}>
              <th style={cell}>event</th>
              <th style={cell}>properties</th>
              <th style={cell}>role</th>
            </tr>
          </thead>
          <tbody>
            {events.map((ev) => (
              <tr key={ev.name}>
                <td style={cell}>
                  <code>{ev.name}</code>
                </td>
                <td style={cell}>
                  <code>{ev.props}</code>
                </td>
                <td style={{ ...cell, color: "rgba(255,255,255,0.8)" }}>{ev.role}</td>
              </tr>
            ))}
          </tbody>
        </table>

        <p style={{ marginTop: 24, color: "rgba(255,255,255,0.8)" }}>
          <strong>cast_not_found_recovery_rate</strong> ={" "}
          <code>(cast_not_found_go_home + cast_not_found_view_docs) / cast_not_found_shown</code>
        </p>
        <p style={{ marginTop: 8, color: "rgba(255,255,255,0.6)", fontSize: "0.9rem" }}>
          Split <code>cast_not_found_shown</code> by <code>{"{ from, reason }"}</code>{" "}
          to size where the leak comes from (streamer vs watcher) and why
          (missing / malformed / expired / ended).
        </p>

        <p style={{ marginTop: 24 }}>
          <a
            href="/landings/cast-not-found"
            style={{ color: "#fff", textDecoration: "underline" }}
          >
            {"\u{2190}"} Back to the 404 fallback
          </a>
        </p>
      </div>
    </main>
  );
}
