export type MpLane = "protocol" | "engine" | "mixed";
export type MpScenario = "burst" | "churn" | "soak" | "fuzz";
export type MpSourceKind = "fixture" | "dir" | "realm";

export const LANES: MpLane[] = ["protocol", "engine", "mixed"];

export const LANE_LABELS: Record<MpLane, string> = {
  protocol: "Protocol",
  engine: "Engine",
  mixed: "Mixed",
};

export const LANE_CAPS: Record<MpLane, { min: number; max: number }> = {
  protocol: { min: 2, max: 8 },
  engine: { min: 2, max: 3 },
  mixed: { min: 2, max: 3 },
};

export const GAME_FIXTURES = [
  "fastlane",
  "cleantheclub",
  "flagtag",
  "skychaser",
] as const;

export const SYNC_FIXTURE = "mp-sync";

export const SCENARIOS: MpScenario[] = ["burst", "churn", "soak", "fuzz"];

export const SCENARIO_LABELS: Record<MpScenario, string> = {
  burst: "Join burst",
  churn: "Churn",
  soak: "Soak",
  fuzz: "Fuzz",
};

export const FALLBACK_PROFILES = [
  "none",
  "lan",
  "latency150",
  "loss5",
  "jitter",
  "burstloss",
  "aa-control",
];

export const WINDOW_LIMITS = { min: 5, max: 3600, fallback: 30 };

export type MpThresholds = {
  max_diverged_final: number;
  max_converge_ms: number;
  min_synced_pct: number;
  max_never_synced: number;
};

export const DEFAULT_THRESHOLDS: MpThresholds = {
  max_diverged_final: 0,
  max_converge_ms: 10_000,
  min_synced_pct: 100,
  max_never_synced: 0,
};

export const LANE_CLAIMS: Record<MpLane, string> = {
  protocol:
    "Full SDK scene sync, measured on the wire: a hammurabi runner plus scripted bots in a private LiveKit room. No engine pixels — nothing here proves rendering.",
  engine:
    "Presence and movement only — SDK scene CRDT never rides ws-room, so this lane cannot claim scene-state sync. Beacons prove scene-execution liveness (and act as a no-sync negative control); shots and fps come from real native clients.",
  mixed:
    "Full sync for the runner and bots; native engine peers are receive-only for scene CRDT and observed via beacons only. Shots and fps come from real native clients.",
};

export const LANE_STARTING_NOTE =
  "First runner boot takes 40–100 s — this is normal, not a hang.";

export const SIDECAR_START_COMMAND =
  "cd tools/mp-testd && npm run build && npm start";

export function laneCap(lane: MpLane): number {
  return LANE_CAPS[lane].max;
}

export function fixtureAllowed(lane: MpLane, fixture: string): boolean {
  if (fixture === SYNC_FIXTURE) return true;
  if ((GAME_FIXTURES as readonly string[]).includes(fixture)) {
    return lane === "protocol";
  }
  return true;
}

export function fixturesForLane(lane: MpLane): string[] {
  return lane === "protocol"
    ? [SYNC_FIXTURE, ...GAME_FIXTURES]
    : [SYNC_FIXTURE];
}

export type MpScene = { kind: MpSourceKind; ref: string };

export type MpShape = {
  profile: string;
  peers?: Record<string, Record<string, number>>;
  schedule?: Array<{
    at_ms: number;
    peer: string;
    partition?: { duration_ms: number };
  }>;
  seed?: number;
};

export type MpRunSpec = {
  lane: MpLane;
  scene: MpScene;
  bots: number;
  mode: MpScenario;
  window?: number;
  shape?: MpShape;
  thresholds?: MpThresholds;
};

export type MpLaunchRequest = { preset: string } | MpRunSpec;

export type MpSpecCheck =
  | { ok: true }
  | { ok: false; field: string; error: string };

export function validateSpec(req: MpLaunchRequest): MpSpecCheck {
  if ("preset" in req) {
    return req.preset.trim()
      ? { ok: true }
      : { ok: false, field: "preset", error: "Pick a preset." };
  }
  const spec = req;
  if (!LANES.includes(spec.lane)) {
    return { ok: false, field: "lane", error: "Unknown lane." };
  }
  const caps = LANE_CAPS[spec.lane];
  if (!Number.isInteger(spec.bots) || spec.bots < caps.min || spec.bots > caps.max) {
    return {
      ok: false,
      field: "bots",
      error: `${LANE_LABELS[spec.lane]} lane runs ${caps.min}–${caps.max} clients.`,
    };
  }
  if (!spec.scene || !spec.scene.ref?.trim()) {
    return { ok: false, field: "scene", error: "Pick a scene source." };
  }
  if (!["fixture", "dir", "realm"].includes(spec.scene.kind)) {
    return { ok: false, field: "scene", error: "Unknown scene source." };
  }
  if (spec.scene.kind === "fixture" && !fixtureAllowed(spec.lane, spec.scene.ref)) {
    return {
      ok: false,
      field: "scene",
      error: `${spec.scene.ref} is protocol-lane only until an engine-lane ledger row proves it against native bevy.`,
    };
  }
  if (!SCENARIOS.includes(spec.mode)) {
    return { ok: false, field: "mode", error: "Unknown scenario." };
  }
  if (
    spec.window !== undefined &&
    (!Number.isInteger(spec.window) ||
      spec.window < WINDOW_LIMITS.min ||
      spec.window > WINDOW_LIMITS.max)
  ) {
    return {
      ok: false,
      field: "window",
      error: `Window is ${WINDOW_LIMITS.min}–${WINDOW_LIMITS.max} seconds.`,
    };
  }
  const t = spec.thresholds;
  if (t) {
    for (const [k, v] of Object.entries(t)) {
      if (!Number.isFinite(v) || v < 0) {
        return { ok: false, field: k, error: "Thresholds must be numbers ≥ 0." };
      }
    }
    if (t.min_synced_pct > 100) {
      return { ok: false, field: "min_synced_pct", error: "min synced % is capped at 100." };
    }
  }
  return { ok: true };
}
