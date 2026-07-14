import { useCallback, useEffect, useRef, useState } from "react";
import { useMachine } from "@xstate/react";

import MpLaunchForm from "@ui/creatorhub/mp/MpLaunchForm";
import MpLiveRunView from "@ui/creatorhub/mp/MpLiveRunView";
import MpReportView, { type MpPerfSeries } from "@ui/creatorhub/mp/MpReportView";
import MpReplayDialog from "@ui/creatorhub/mp/MpReplayDialog";
import MpUnpaired from "@ui/creatorhub/mp/MpUnpaired";
import type { MpLane, MpLaunchRequest } from "@ui/creatorhub/mp/rules";
import {
  MpSidecar,
  readPairing,
  type MpConnState,
  type MpPairing,
} from "@ui/creatorhub/mp/sidecar";
import {
  normalizeBot,
  normalizeReport,
  normalizeRunSummary,
  normalizeStatus,
  normalizeVerdict,
  type MpBotRow,
  type MpEventFrame,
  type MpReportData,
  type MpRunSummary,
  type MpTimelineEntry,
  type MpVerdict,
} from "@ui/creatorhub/mp/types";

import { isDesktopShell } from "@data/lib/auth/native-shell";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";
import { track, trackExposure, type TrackContext } from "@core/lib/telemetry/track";
import {
  mpPanelMachine,
  type MpTrackFn,
} from "@features/stories/creator-hub/multiplayer-test/machine";

import type { Route } from "./+types/create.multiplayer-test";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("Multiplayer Test");

const STORY: StoryId = "creator-hub/multiplayer-test";

const FALLBACK: Assignment = {
  variant: "panel",
  flags: { panel: true },
  experimentKey: "ch_mp_scene_testing",
};

const LOG_CAP = 200;
const SHOT_CAP = 12;

const mpTrack = track as unknown as MpTrackFn;

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, assignment, wrap } = await storyLoader(request, STORY, FALLBACK);
  trackExposure({
    sid,
    story: STORY,
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
  });
  return wrap({
    sid,
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
  });
}

export default function MultiplayerTest({ loaderData }: Route.ComponentProps) {
  const { sid, variant, experimentKey } = loaderData;
  return (
    <MpTestPanel
      trackCtx={{ sid, story: STORY, variant, experimentKey }}
    />
  );
}

function MpTestPanel({ trackCtx }: { trackCtx: TrackContext }) {
  const clientRef = useRef<MpSidecar | null>(null);
  const [pairing, setPairing] = useState<MpPairing | null>(null);
  const [conn, setConn] = useState<MpConnState>("down");
  const [presets, setPresets] = useState<string[]>([]);
  const [profiles, setProfiles] = useState<string[]>([]);
  const [recentRuns, setRecentRuns] = useState<MpRunSummary[]>([]);
  const [lane, setLane] = useState<MpLane>("protocol");
  const [bots, setBots] = useState<Record<string, MpBotRow>>({});
  const histRef = useRef(new Map<string, MpTimelineEntry[]>());
  const [timeline, setTimeline] = useState<MpTimelineEntry[]>([]);
  const [logs, setLogs] = useState<string[]>([]);
  const [shots, setShots] = useState<string[]>([]);
  const [report, setReport] = useState<MpReportData | null>(null);
  const [verdict, setVerdict] = useState<MpVerdict | null>(null);
  const [perf, setPerf] = useState<MpPerfSeries[]>([]);
  const [shellOnHttps, setShellOnHttps] = useState(false);

  const [state, send, actorRef] = useMachine(mpPanelMachine, {
    input: {
      trackCtx,
      track: mpTrack,
      launch: (req: MpLaunchRequest) => {
        const c = clientRef.current;
        if (!c) return Promise.reject(new Error("sidecar not paired"));
        return c.launch(req);
      },
      replay: async ({ runId, tier, profile, seed }) => {
        const c = clientRef.current;
        if (!c) throw new Error("sidecar not paired");
        const resp = (await c.replay(runId, { tier, profile, seed })) as {
          replay?: string;
          id?: string;
        } | null;
        if (tier === "b") return resp;
        const replayId = resp?.replay ?? resp?.id;
        if (!replayId) throw new Error("sidecar returned no replay id");
        for (let i = 0; i < 30; i++) {
          const outcome = await c
            .artifactJson<Record<string, unknown>>(
              runId,
              `replay/${replayId}/outcome.json`,
            )
            .catch(() => null);
          if (outcome) return { replay: replayId, ...outcome };
          await new Promise((res) => setTimeout(res, 2_000));
        }
        throw new Error(
          `replay ${replayId} produced no outcome.json \u{2014} is mp-analyzer installed?`,
        );
      },
      stop: (runId: string) => {
        void clientRef.current?.stop(runId).catch(() => {});
      },
    },
  });

  const runId = state.context.runId;
  const phase = state.context.phase;

  const refreshCatalog = useCallback(async () => {
    const c = clientRef.current;
    if (!c) return;
    try {
      const [p, pr, rs] = await Promise.all([
        c.presets().catch(() => null),
        c.profiles().catch(() => null),
        c.runs().catch(() => null),
      ]);
      if (p) {
        setPresets(
          p
            .map((x) =>
              typeof x === "string"
                ? x
                : ((x as { name?: string })?.name ?? null),
            )
            .filter((x): x is string => !!x),
        );
      }
      if (pr) {
        setProfiles(
          pr
            .map((x) =>
              typeof x === "string"
                ? x
                : ((x as { name?: string })?.name ?? null),
            )
            .filter((x): x is string => !!x),
        );
      }
      if (rs) {
        setRecentRuns(
          rs
            .map(normalizeRunSummary)
            .filter((x): x is MpRunSummary => x !== null)
            .slice(0, 8),
        );
      }
    } catch {
    }
  }, []);

  useEffect(() => {
    setShellOnHttps(isDesktopShell() && window.location.protocol === "https:");
    const p = readPairing(window);
    setPairing(p);
    if (!p) return;
    const client = new MpSidecar(p);
    clientRef.current = client;
    client.connect({
      onState: (s) => {
        setConn(s);
        if (s === "connected") {
          send({ type: "PAIRED" });
          void refreshCatalog();
        } else if (s === "down") {
          const v = actorRef.getSnapshot().value;
          if (v === "idle" || v === "unpaired") send({ type: "PAIR_LOST" });
        }
      },
      onEvent: (frame: MpEventFrame) => {
        const ctxRun = actorRef.getSnapshot().context.runId;
        if (frame.type === "status") {
          const st = normalizeStatus(frame);
          const frameRun = typeof frame.run === "string" ? frame.run : null;
          if (!st || !frameRun) return;
          const hist = histRef.current;
          const entries = hist.get(frameRun) ?? [];
          if (!entries.length || entries[entries.length - 1].state !== st.state) {
            entries.push({ state: st.state, detail: st.detail, at: Date.now() });
            hist.set(frameRun, entries);
            if (hist.size > 8) {
              const first = hist.keys().next().value;
              if (first !== undefined && first !== frameRun) hist.delete(first);
            }
          }
          if (frameRun !== ctxRun) return;
          setTimeline([...entries]);
          send({ type: "STATUS", state: st.state, detail: st.detail });
          return;
        }
        if (frame.run && ctxRun && frame.run !== ctxRun) return;
        if (frame.type === "bot") {
          const row = normalizeBot(frame);
          if (row) setBots((m) => ({ ...m, [row.peer]: row }));
        } else if (frame.type === "log") {
          const line =
            typeof frame.line === "string"
              ? frame.line
              : typeof frame.msg === "string"
                ? frame.msg
                : null;
          if (line !== null) {
            setLogs((ls) => [...ls.slice(-(LOG_CAP - 1)), line]);
          }
        }
      },
    });
    return () => {
      client.close();
      clientRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!runId || !clientRef.current) return;
    let cancelled = false;
    void clientRef.current
      .run(runId)
      .then((r) => {
        const l = (r as { lane?: string } | null)?.lane;
        if (!cancelled && (l === "protocol" || l === "engine" || l === "mixed")) {
          setLane(l);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [runId]);

  const launching = state.matches("launching");
  const running = state.matches("running");
  const reviewing = state.matches("reviewing");

  useEffect(() => {
    if (!launching) return;
    setBots({});
    setLogs([]);
    setShots([]);
    setReport(null);
    setVerdict(null);
    setPerf([]);
    setTimeline([]);
  }, [launching]);

  useEffect(() => {
    if (!runId) return;
    setTimeline([...(histRef.current.get(runId) ?? [])]);
  }, [runId]);

  useEffect(() => {
    if (!running || !runId || (lane !== "engine" && lane !== "mixed")) return;
    const c = clientRef.current;
    if (!c) return;
    let cancelled = false;
    const seen = new Set<string>();
    const poll = async () => {
      try {
        const listing = await c.artifactEntries(runId, "shots");
        if (cancelled) return;
        for (const entry of listing) {
          const name = entry.dir ? null : entry.name;
          if (!name || seen.has(name)) continue;
          seen.add(name);
          const url = await c.artifactBlobUrl(runId, `shots/${name}`);
          if (url && !cancelled) {
            setShots((s) => [...s.slice(-(SHOT_CAP - 1)), url]);
          }
        }
      } catch {
      }
    };
    void poll();
    const t = setInterval(poll, 5_000);
    return () => {
      cancelled = true;
      clearInterval(t);
    };
  }, [running, runId, lane]);

  useEffect(() => {
    if (!reviewing || !runId) return;
    const c = clientRef.current;
    if (!c) return;
    let cancelled = false;
    let tries = 0;
    const poll = async () => {
      tries += 1;
      try {
        const [r, v] = await Promise.all([
          c.report(runId).catch(() => null),
          c.verdict(runId).catch(() => null),
        ]);
        if (cancelled) return true;
        const nr = normalizeReport(r);
        const nv = normalizeVerdict(v);
        if (nr) setReport(nr);
        if (nv) setVerdict(nv);
        return !!(nr && nv);
      } catch {
        return false;
      }
    };
    void (async () => {
      while (!cancelled && tries < 20) {
        if (await poll()) break;
        await new Promise((res) => setTimeout(res, 3_000));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [reviewing, runId]);

  useEffect(() => {
    if (!reviewing || !runId || (lane !== "engine" && lane !== "mixed")) return;
    const c = clientRef.current;
    if (!c) return;
    let cancelled = false;
    void (async () => {
      const series: MpPerfSeries[] = [];
      for (const peer of Object.keys(bots)) {
        const csv = await c.artifactText(runId, `perf/${peer}.csv`).catch(() => null);
        if (!csv) continue;
        const fps = csv
          .split("\n")
          .map((l) => Number(l.split(",").pop()))
          .filter((n) => Number.isFinite(n));
        if (fps.length) series.push({ client: peer, fps });
      }
      if (!cancelled && series.length) setPerf(series);
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reviewing, runId, lane]);

  const openReportHtml = useCallback(() => {
    const c = clientRef.current;
    if (!c || !runId) return;
    void c
      .artifactBlobUrl(runId, "metrics/report.html")
      .then((url) => {
        if (url) window.open(url, "_blank", "noopener");
      })
      .catch(() => {});
  }, [runId]);

  const panelState = state.matches("unpaired")
    ? "unpaired"
    : launching
      ? "launching"
      : running
        ? "running"
        : reviewing
          ? "reviewing"
          : "idle";

  const devOriginUrl =
    typeof window !== "undefined"
      ? `http://localhost:5197${window.location.pathname}`
      : undefined;

  return (
    <main style={WRAP} data-mp-state={panelState}>
      <header style={HEAD}>
        <h1 style={TITLE}>Multiplayer Test</h1>
        <p style={SUBTITLE}>
          Spawn measured clients against your scene, watch sync live, and get
          an honest pass/fail sync-health report.
        </p>
      </header>

      {panelState === "unpaired" && (
        <MpUnpaired
          reason={pairing ? "unreachable" : "no-pairing"}
          port={pairing?.port ?? null}
          desktopShellOnHttps={shellOnHttps}
          devOriginUrl={devOriginUrl}
        />
      )}

      {(panelState === "idle" || panelState === "launching") && (
        <>
          {conn !== "connected" && (
            <p style={BANNER} role="status">
              Sidecar connection {conn === "connecting" ? "starting" : "lost"} &#x2014;
              retrying.
            </p>
          )}
          <MpLaunchForm
            presets={presets}
            profiles={profiles}
            disabled={launching || conn !== "connected"}
            error={state.context.lastError}
            onLaunch={(req) => send({ type: "LAUNCH", spec: req })}
          />
          {recentRuns.length > 0 && (
            <section style={RECENT} aria-label="Recent runs">
              <h2 style={RECENT_TITLE}>Recent runs</h2>
              <ul style={RECENT_LIST}>
                {recentRuns.map((r) => (
                  <li key={r.id} style={RECENT_ROW}>
                    <span style={RECENT_ID}>{r.id}</span>
                    <span style={RECENT_META}>
                      {[r.lane, r.state, r.created].filter(Boolean).join(" \u{B7} ")}
                    </span>
                    <button
                      type="button"
                      style={RECENT_BTN}
                      data-mp-action="review-run"
                      data-mp-run={r.id}
                      onClick={() => send({ type: "SELECT_RUN", runId: r.id })}
                    >
                      Review
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          )}
        </>
      )}

      {panelState === "running" && runId && phase && (
        <MpLiveRunView
          runId={runId}
          lane={lane}
          phase={phase}
          detail={state.context.detail}
          bots={Object.values(bots)}
          timeline={timeline}
          logs={logs}
          shots={shots}
          connLost={conn !== "connected"}
          onStop={() => send({ type: "STOP" })}
        />
      )}

      {panelState === "reviewing" && runId && (
        <>
          <MpReportView
            runId={runId}
            lane={lane}
            report={report}
            verdict={verdict}
            timeline={timeline}
            perf={perf}
            onOpenReportHtml={openReportHtml}
            onOpenReplay={() => send({ type: "OPEN_REPLAY" })}
            onNewRun={() => {
              send({ type: "NEW_RUN" });
              void refreshCatalog();
            }}
          />
          {state.matches({ reviewing: "replay" }) && (
            <MpReplayDialog
              runId={runId}
              profiles={profiles}
              requesting={state.matches({ reviewing: { replay: "requesting" } })}
              outcome={state.context.replayOutcome}
              error={state.context.replayError}
              onSubmit={({ tier, profile, seed }) =>
                send({ type: "REPLAY", tier, profile, seed })
              }
              onClose={() => send({ type: "CLOSE_REPLAY" })}
            />
          )}
        </>
      )}
    </main>
  );
}

const WRAP: React.CSSProperties = {
  maxWidth: 1080,
  margin: "0 auto",
  padding: "28px 20px 60px",
};
const HEAD: React.CSSProperties = { marginBottom: 20 };
const TITLE: React.CSSProperties = { margin: 0, fontSize: 26, lineHeight: 1.25 };
const SUBTITLE: React.CSSProperties = {
  margin: "6px 0 0",
  color: "var(--ink-7)",
  fontSize: 14,
  lineHeight: 1.5,
};
const BANNER: React.CSSProperties = {
  margin: "0 0 14px",
  padding: "8px 12px",
  border: "1px solid var(--line)",
  borderRadius: "var(--r-control)",
  background: "var(--fill-1)",
  color: "var(--ink-7)",
  fontSize: 13,
};
const RECENT: React.CSSProperties = {
  marginTop: 20,
  padding: "12px 14px",
  border: "1px solid var(--line)",
  borderRadius: "var(--r-card)",
  background: "var(--panel)",
};
const RECENT_TITLE: React.CSSProperties = {
  margin: "0 0 8px",
  fontSize: 12,
  fontWeight: 600,
  letterSpacing: 0.3,
  textTransform: "uppercase",
  color: "var(--ink-7)",
};
const RECENT_LIST: React.CSSProperties = {
  margin: 0,
  padding: 0,
  listStyle: "none",
  display: "flex",
  flexDirection: "column",
  gap: 6,
};
const RECENT_ROW: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 10,
  fontSize: 13,
};
const RECENT_ID: React.CSSProperties = {
  fontFamily: "var(--mono, monospace)",
  fontSize: 12,
};
const RECENT_META: React.CSSProperties = { color: "var(--ink-7)", flex: 1 };
const RECENT_BTN: React.CSSProperties = {
  padding: "4px 12px",
  border: "1px solid var(--line)",
  borderRadius: "var(--r-control)",
  background: "var(--fill-2)",
  color: "var(--text)",
  fontSize: 12,
  cursor: "pointer",
};
