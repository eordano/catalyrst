// Foundry v3 types. Every shape here describes something that was really
// recorded: a Worlds deployment entity, a design document someone wrote, a bot
// run that executed, a token count the gateway reported. There is no seeded,
// simulated or projected variant of any of these.

// Where a row came from. `import` = read out of a real external source (the
// worlds mirror, a vendored design doc); `recorded` = produced by an execution
// we ran (bot runs, llm usage); `visitor` = someone on this site did it.
export type Origin = "import" | "visitor" | "recorded";

export type SceneSource = "worlds-mirror" | "repo";

export interface FoundryScene {
  id: string;
  title: string;
  worldName: string | null;
  entityId: string | null;
  deployedAt: string | null;
  sizeBytes: number | null;
  parcels: number | null;
  repoPath: string | null;
  botManifest: string | null;
  source: SceneSource;
  sourceNote: string;
  gddDocId: string | null;
  // When this row was written from the worlds mirror — the honest "as of" for
  // every deployment fact on it, since nothing re-reads the mirror on load.
  importedAt: string | null;
}

export interface ChangelogRow {
  at: string;
  note: string;
  sourceNote: string;
  origin: Origin;
}

export type GddKind = "shortgdd" | "proposal" | "brief" | "feature-design";

export type GddHypothesisStatus =
  | "parked"
  | "active"
  | "validated"
  | "survived"
  | "failed"
  | "deferred";

export interface GddHypothesis {
  id: string;
  stage: string;
  slug: string;
  status: GddHypothesisStatus;
  ifThen?: string;
  test?: string;
  testedOn?: string;
}

export interface GddHonestySection {
  name: string;
  open: number;
  tbd: number;
  hypothesis: number;
  agentDecided: number;
}

export interface GddHonestyTotals {
  open: number;
  tbd: number;
  hypothesis: number;
  agentDecided: number;
}

export interface GddHonesty {
  sections: GddHonestySection[];
  totals: GddHonestyTotals;
}

export type GddSource = "slack-import" | "copilot";

export interface GddDoc {
  id: string;
  title: string;
  kind: GddKind;
  sceneId: string | null;
  version: number;
  supersedes: string | null;
  source: GddSource;
  sourceRef: string | null;
  bodyMd: string;
  honesty: GddHonesty;
  hypotheses: GddHypothesis[];
  createdAt: string;
  updatedAt: string;
}

export interface LlmUsageRow {
  messageId: string;
  sessionId: string;
  sessionTitle: string | null;
  model: string;
  inputTokens: number;
  outputTokens: number;
  reasoningTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  costUsd: number | null;
  priceInputPerM: number;
  priceOutputPerM: number;
  at: string;
}

export interface LlmUsageDay {
  day: string;
  inputTokens: number;
  outputTokens: number;
  costUsd: number;
}

export interface LlmUsageSummary {
  messages: number;
  sessions: number;
  inputTokens: number;
  outputTokens: number;
  costUsd: number;
  byDay: LlmUsageDay[];
  recent: LlmUsageRow[];
}

// Tokens are measured by the gateway. The dollar figure is not a bill: the
// model is self-hosted, so a price has to be chosen before a cost can be
// stated. This is that chosen constant, and it travels with its own label so no
// surface can quote the number without the caveat.
export const LLM_REFERENCE_PRICING = {
  inputPerM: 0.1,
  outputPerM: 0.3,
  label:
    "reference pricing — $0.10 in / $0.30 out per 1M tokens; tokens are measured, the price is a chosen constant (self-hosted)",
} as const;

export type TrajectoryEventType =
  | "turn/start"
  | "turn/end"
  | "step/start"
  | "step/end"
  | "tool/call"
  | "tool/result"
  | "obs/snapshot"
  | "check/verdict"
  | "run/end-seed";

export interface TurnEndReason {
  kind: "completed" | "aborted" | "error" | "max-steps" | "interrupted";
  detail?: string;
}

export interface TrajectoryEvent {
  trajectoryId: string;
  seq: number;
  type: TrajectoryEventType;
  time: string;
  data: unknown;
  ignorable?: true;
}

export type TrajectoryProvenance = "bot" | "visitor";
export type TrajectoryRunner = "dclbots" | "arena";

export interface Trajectory {
  id: string;
  sceneId: string | null;
  provenance: TrajectoryProvenance;
  runner: TrajectoryRunner | null;
  finishReason: TurnEndReason | null;
  parentTrajectoryId: string | null;
  seedLength: number | null;
  evidencePath: string | null;
  createdAt: string;
}

export interface BotReport {
  id: string;
  sceneId: string | null;
  slug: string;
  // 'arena' is a self-contained sandbox simulation; 'dclbots' is a run against a
  // scene copy in a real explorer. The distinction is the difference between a
  // simulated game and a tested one, so it is carried, never inferred.
  runner: TrajectoryRunner | null;
  // The realm the run actually targeted. Bench runs to date carry a loopback
  // realm (a local copy of the scene), never the deployed World.
  realm: string | null;
  ranAt: string;
  verdict: "pass" | "fail" | null;
  checksTotal: number | null;
  checksFailed: number | null;
  missingTools: string[];
  stubbedTools: string[];
  networkWrites: number | null;
  shots: string[];
  evidencePath: string | null;
  trajectoryId: string | null;
}

export interface CheckVerdict {
  kind: string;
  pass: boolean;
  detail: string;
  why: string;
}

export interface RequestBoardRow {
  id: string;
  title: string;
  body: string;
  source: string;
  status: "open" | "approved" | "closed";
  pledges: number;
  pledgedByMe: boolean;
  origin: Origin;
  createdAt: string;
}

export interface ExchangeStats {
  openRequests: number;
  totalPledges: number;
}

export interface ActivityRow {
  id: number;
  at: string;
  actor: string;
  action: string;
  subject: string;
  detail: Record<string, unknown>;
  mine: boolean;
}

// The import fixture: the registry as read out of the worlds mirror plus the
// one scene that lives in this repository. `generatedFrom` is the provenance of
// the file itself — the surface quotes it rather than claiming the numbers as
// its own.
export interface FoundryRealSceneRow {
  id: string;
  title: string;
  worldName: string | null;
  entityId: string | null;
  deployedAt: string | null;
  sizeBytes: number | null;
  parcels: number | null;
  repoPath: string | null;
  botManifest: string | null;
  source: SceneSource;
  sourceNote: string;
}

export interface FoundryRealFixture {
  generatedFrom: {
    source: string;
    readAt: string;
    query: string;
    notes: string[];
  };
  scenes: FoundryRealSceneRow[];
}

export interface ImportSummary {
  scenes: number;
  changelog: number;
}
