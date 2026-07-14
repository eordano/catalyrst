import type { MpLane } from "./rules";

export type MpRunPhase =
  | "queued"
  | "starting"
  | "running"
  | "analyzing"
  | "done"
  | "failed";

export type MpRunStatus = {
  state: MpRunPhase;
  detail?: string;
  updated?: string;
};

export type MpEventFrame = {
  type: string;
  run?: string;
  [k: string]: unknown;
};

export type MpBotRow = {
  peer: string;
  connected: boolean;
  deltas: number;
  synced: boolean | null;
  probe: string | null;
};

export type MpTimelineEntry = { state: string; detail?: string; at: number };

export type MpRunSummary = {
  id: string;
  created?: string;
  lane?: MpLane;
  state?: string;
};

export type MpVerdictCheck = {
  name: string;
  value: number;
  threshold: number;
  pass: boolean;
};

export type MpVerdict = { pass: boolean; checks: MpVerdictCheck[] };

export type MpDivergenceRow = {
  entity: string;
  component: string;
  peers: string[];
  closed: boolean | null;
  oracle: string | null;
  beacon: string | null;
  probe: string | null;
};

export type MpReportData = {
  convergeMedianMs: number | null;
  convergeP95Ms: number | null;
  perBotConvergeMs: Record<string, number>;
  corrections: number | null;
  joinRetriesMax: number | null;
  neverSynced: string[];
  divergence: MpDivergenceRow[];
  swarmVerdict: string | null;
};

export type MpReplayOutcome = {
  id: string | null;
  tier: "a" | "b";
  hash: string | null;
  decisionHash: string | null;
};

const num = (v: unknown): number | null =>
  typeof v === "number" && Number.isFinite(v) ? v : null;

const str = (v: unknown): string | null =>
  typeof v === "string" && v ? v : null;

const rec = (v: unknown): Record<string, unknown> | null =>
  v && typeof v === "object" && !Array.isArray(v)
    ? (v as Record<string, unknown>)
    : null;

const PHASES: MpRunPhase[] = [
  "queued",
  "starting",
  "running",
  "analyzing",
  "done",
  "failed",
];

export function normalizeStatus(v: unknown): MpRunStatus | null {
  const o = rec(v);
  if (!o) return null;
  const state = str(o.state);
  if (!state || !PHASES.includes(state as MpRunPhase)) return null;
  return {
    state: state as MpRunPhase,
    detail: str(o.detail) ?? undefined,
    updated: str(o.updated) ?? undefined,
  };
}

export function normalizeBot(v: unknown): MpBotRow | null {
  const o = rec(v);
  if (!o) return null;
  const peer = str(o.bot) ?? str(o.peer) ?? str(o.id);
  if (!peer) return null;
  const f = rec(o.fields) ?? o;
  const truthy = (x: unknown): boolean | null =>
    typeof x === "boolean" ? x : typeof x === "number" ? x > 0 : null;
  return {
    peer,
    connected: truthy(f.connected) ?? false,
    deltas: num(f.srv_crdt) ?? num(f.deltas) ?? num(f.crdt_deltas) ?? 0,
    synced: truthy(f.synced) ?? truthy(f.crdt_synced),
    probe: str(f.probe) ?? str(f.probe_hash),
  };
}

export function normalizeVerdict(v: unknown): MpVerdict | null {
  const o = rec(v);
  if (!o || typeof o.pass !== "boolean") return null;
  const checks: MpVerdictCheck[] = [];
  if (Array.isArray(o.checks)) {
    for (const c of o.checks) {
      const co = rec(c);
      if (!co) continue;
      const name = str(co.name);
      if (!name) continue;
      checks.push({
        name,
        value: num(co.value) ?? 0,
        threshold: num(co.threshold) ?? 0,
        pass: co.pass === true,
      });
    }
  }
  return { pass: o.pass, checks };
}

export function normalizeRunSummary(v: unknown): MpRunSummary | null {
  const o = rec(v);
  if (!o) return null;
  const id = str(o.id) ?? str(o.run);
  if (!id) return null;
  return {
    id,
    created: str(o.created) ?? undefined,
    lane: (str(o.lane) as MpLane | null) ?? undefined,
    state: str(o.state) ?? undefined,
  };
}

function divergenceRows(v: unknown, closed: boolean | null): MpDivergenceRow[] {
  if (!Array.isArray(v)) return [];
  const rows: MpDivergenceRow[] = [];
  for (const e of v) {
    const o = rec(e);
    if (!o) continue;
    const entity = str(o.entity) ?? (num(o.entity) !== null ? String(o.entity) : null);
    const component = str(o.component);
    if (!entity || !component) continue;
    rows.push({
      entity,
      component,
      peers: Array.isArray(o.peers) ? o.peers.map(String) : [],
      closed: typeof o.closed === "boolean" ? o.closed : closed,
      oracle: str(o.oracle),
      beacon: str(o.beacon),
      probe: str(o.probe) ?? str(o.runner_store),
    });
  }
  return rows;
}

export function normalizeReport(v: unknown): MpReportData | null {
  const o = rec(v);
  if (!o) return null;
  const conv = rec(o.convergence) ?? {};
  const perBotRaw = rec(conv.per_bot) ?? rec(conv.perBot) ?? {};
  const perBot: Record<string, number> = {};
  for (const [k, val] of Object.entries(perBotRaw)) {
    const n = num(val);
    if (n !== null) perBot[k] = n;
  }
  const div = rec(o.divergence) ?? {};
  const corr = rec(o.corrections) ?? {};
  const join = rec(o.join) ?? {};
  const retries = rec(join.retries) ?? {};
  let retriesMax: number | null = null;
  for (const val of Object.values(retries)) {
    const n = num(val);
    if (n !== null) retriesMax = Math.max(retriesMax ?? 0, n);
  }
  return {
    convergeMedianMs: num(conv.median_ms) ?? num(conv.medianMs),
    convergeP95Ms: num(conv.p95_ms) ?? num(conv.p95Ms),
    perBotConvergeMs: perBot,
    corrections: num(corr.total) ?? num(o.corrections),
    joinRetriesMax: retriesMax,
    neverSynced: Array.isArray(div.never_synced)
      ? div.never_synced.map(String)
      : [],
    divergence: [
      ...divergenceRows(div.final, false),
      ...divergenceRows(div.transient ?? div.checkpoints, true),
    ],
    swarmVerdict: str(o.swarm_verdict),
  };
}

export function normalizeReplayOutcome(
  v: unknown,
  tier: "a" | "b",
): MpReplayOutcome {
  const o = rec(v) ?? {};
  const outcome = rec(o.outcome) ?? {};
  return {
    id: str(o.id) ?? str(o.replay) ?? str(o.replay_id),
    tier,
    hash:
      str(outcome.converged_hash) ??
      str(outcome.state_hash) ??
      str(outcome.hash) ??
      str(o.converged_hash) ??
      str(o.state_hash) ??
      str(o.hash),
    decisionHash: str(outcome.decision_hash) ?? str(o.decision_hash),
  };
}
