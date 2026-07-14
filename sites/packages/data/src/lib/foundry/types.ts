// Foundry v3 types. Every shape here describes something that was really
// recorded: a Worlds deployment entity, a design document someone wrote, a bot
// run that executed, a token count the gateway reported. There is no seeded,
// simulated or projected variant of any of these.

// Where a row came from. `import` = read out of a real external source (the
// worlds mirror, a vendored design doc); `recorded` = produced by an execution
// we ran (bot runs, llm usage); `visitor` = someone on this site did it.
export type Origin = "import" | "visitor" | "recorded";

export type SceneSource = "worlds-mirror" | "repo";

// The three gaming cells of the strategy deck's slide "09 | MARKET-CELL
// PORTFOLIO" (11.10, Scott McCarthy working-strategy deck).
export type MarketCellSlug =
  | "creator-led-social-competition"
  | "community-operated-game-clubs"
  | "collaborative-build-and-play-labs";

export type MarketCellConfidence = "evidence-backed" | "inferred";

// This program's own reading of one game against the deck's cells — a curated
// judgment made on classifiedAt, never a fact the deployment entity carries.
// `cell` NULL means "examined and honestly unclassifiable"; a scene with no
// row at all has simply not been read (FoundryScene.marketCell stays null).
export interface SceneMarketCell {
  cell: MarketCellSlug | null;
  rationale: string;
  confidence: MarketCellConfidence;
  classifiedAt: string;
  basis: string;
}

// The six emotional jobs of the strategy deck's slide "10 | EMOTIONAL WHITE
// SPACE" (11.10, Scott McCarthy working-strategy deck).
export type EmotionalJob = "A" | "B" | "C" | "D" | "E" | "F";

// One row per job the game's observable design serves — this program's own
// reading, made on readAt. `job` null means "read, and honestly serves none of
// the six"; a scene with no rows at all has simply not been read.
export interface SceneEmotionalJobRead {
  job: EmotionalJob | null;
  rationale: string;
  confidence: MarketCellConfidence;
  readAt: string;
  basis: string;
}

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
  // The deployment entity's own scene.json display facts, read from the worlds
  // content server at import; NULL until an import has read them.
  description: string | null;
  thumbnailUrl: string | null;
  // This program's market-cell reading of the game, when one has been made.
  // null = not yet read at all — distinct from a row whose `cell` is null
  // (read, and honestly unclassifiable). No default is fabricated either way.
  marketCell: SceneMarketCell | null;
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

export type GddSource = "slack-import" | "copilot" | "program" | "session";

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
  /** The market cell this doc reads as its ground, when it declares one. */
  groundsCell: string | null;
  /** Ids of the stored asks the doc quotes — keys, never parsed prose. */
  groundingRequestIds: string[];
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
  /** Messages in the deploy pipeline's own verification session (titled
   *  DEPLOY_PROBE_SESSION_TITLE) — counted so the headline totals can say how
   *  much of themselves is the deploy proving the gateway answers, not copilot
   *  drafting. Mirrors the arena-row rule's labeled-not-hidden policy. */
  probeMessages: number;
  probeTokens: number;
  /** Reference-priced cost of the probe messages alone, so a surface can
   *  subtract it from a headline instead of showing dollars beside zero
   *  drafting messages. */
  probeCostUsd: number;
  byDay: LlmUsageDay[];
  recent: LlmUsageRow[];
}

/** The session title the deploy's own gateway probe records its usage under.
 *  Kept in lockstep with the UI-side constant in FdCostsPage. */
export const DEPLOY_PROBE_SESSION_TITLE = "deploy-proof";

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
  /** Failed checks whose stored verdict says "cannot evaluate" — counted as
   *  failed by policy. Derived from the run's own check/verdict events; 0 when
   *  no such event is stored. */
  checksUnevaluable: number;
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

// 0005-society shared shapes. Every field below names something a row can hold
// or an explicit absence; none of these carry a fabricated value. The avatar is
// a spec over the real DCL base-avatar catalog, re-rendered live — never a
// stored image. A raw sid is never one of these fields: an actor is a claimed
// persona name or the honest visitor badge, and callers resolve it before the
// wire.

export interface Persona {
  sid: string;
  displayName: string;
  avatarBodyUrn: string | null;
  avatar: Record<string, unknown>;
  /** Optional self-description, in the visitor's own words (≤280 chars). */
  words: string | null;
  claimedAt: string;
  updatedAt: string;
}

// What listPersonas/personaLabels hand a caller for one visitor. A miss (no
// claimed persona) is `null`, and the caller falls back to sidBadge().
export interface PersonaLabel {
  name: string;
  avatarBodyUrn: string | null;
  avatar: Record<string, unknown>;
}

export type RoleName = "admin" | "host" | "create" | "start";

export interface RoleGrantRow {
  id: number;
  role: RoleName;
  note: string;
  since: string;
  grantedBy: string | null;
  viaInvite: boolean;
}

// One visible roster line: a role holder who has consented to be listed. `actor`
// is a persona name when claimed, otherwise the honest badge.
export interface RosterRow {
  role: RoleName;
  actor: { name: string } | { badge: string };
  since: string;
}

// The lanes a timeline row can come from — one per real row-source it unions.
export type TimelineLane =
  | "community"
  | "exchange"
  | "worlds"
  | "harness"
  | "trajectory"
  | "docs";

// One remembered event. `actor` resolves to a persona name, a visitor badge, or
// a source label ('worlds mirror', a bot runner) — a raw sid never reaches here.
export interface TimelineRow {
  lane: TimelineLane;
  id: string;
  at: string;
  actor: { name: string } | { badge: string } | { source: string };
  action: string;
  subject: string | null;
  subjectLabel: string | null;
  /** The table the subject id lives in, resolved only for community rows —
   *  every other lane's subject table is implied by the lane itself. */
  subjectKind: "request" | "scene" | "session" | "doc" | null;
  body: string;
  provenance: Origin | "bot";
  runner: string | null;
  /** The source recorded only a DAY for this event (stored fact, set at
   *  import) — the UI stamps the day alone instead of an invented midnight. */
  dateOnly: boolean;
}

// A single derived occurrence of a session series (never materialized).
export interface SessionOccurrence {
  seriesId: string;
  title: string;
  body: string;
  sceneId: string | null;
  sceneTitle: string | null;
  cadence: "once" | "weekly";
  occurrenceAt: string;
  durationMinutes: number;
  host: { name: string } | { badge: string };
  rsvpCount: number;
  viewerRsvped: boolean;
  label: string;
}

export interface SessionSeriesInput {
  title: string;
  body: string;
  sceneId: string | null;
  cadence: "once" | "weekly";
  firstAt: string;
  durationMinutes: number;
}

export type ConsentTopic = "steward-code" | "roster-listing";
export type ConsentState = "granted" | "withdrawn";

// Latest state per topic; a topic never touched is absent from the map.
export interface ConsentSnapshot {
  topics: Partial<Record<ConsentTopic, { state: ConsentState; at: string }>>;
}

// A decision that really happened and touches the viewer — the only subjects an
// appeal can name.
export interface DecisionRow {
  kind: "request" | "role_grant" | "session_series";
  id: string;
  label: string;
  at: string;
  detail: string;
}

export interface AppealRow {
  id: string;
  subjectKind: "request" | "role_grant" | "session_series";
  subjectId: string;
  subjectLabel: string | null;
  body: string;
  status: "open" | "withdrawn" | "upheld" | "declined";
  createdAt: string;
  resolvedBy: { name: string } | { badge: string } | null;
  resolvedAt: string | null;
  resolutionNote: string | null;
  appellant?: { name: string } | { badge: string };
}

export interface SceneStewardRow {
  sid: string;
  actor: { name: string } | { badge: string };
  basis: string;
  since: string;
  releasedAt: string | null;
  releaseReason: "self" | "transfer" | null;
  viaTransfer: boolean;
}

/** A steward row as it may cross the wire: the raw sid never leaves the data
 *  layer — listStewards strips it before returning. */
export type PublicSceneSteward = Omit<SceneStewardRow, "sid">;

export interface SceneTransferRow {
  id: string;
  sceneId: string;
  from: { name: string } | { badge: string };
  note: string;
  status: "offered" | "accepted" | "revoked";
  effectiveStatus: "offered" | "accepted" | "revoked" | "expired";
  createdAt: string;
  expiresAt: string;
  acceptedAt: string | null;
  acceptedBy: { name: string } | { badge: string } | null;
}

// What the /succession/:token accept page shows for one token: either an
// acceptable offer, or the specific honest reason it cannot be accepted.
export interface TransferView {
  sceneId: string;
  sceneTitle: string;
  from: { name: string } | { badge: string };
  note: string;
  expiresAt: string;
  effectiveStatus: "offered" | "accepted" | "revoked" | "expired";
}

// The downloadable project bundle: raw rows assembled by export.server, then
// sanitized at the route edge (token_hash dropped, sids badged, paths labeled).
export interface ProjectBundleProvenance {
  scene: string;
  generatedAt: string;
  note: string;
}

export interface ProjectBundle {
  provenance: ProjectBundleProvenance;
  scene: FoundryScene | null;
  changelog: ChangelogRow[];
  docs: GddDoc[];
  reports: BotReport[];
  trajectories: {
    trajectory: Trajectory;
    events: Omit<TrajectoryEvent, "trajectoryId">[];
    eventsStored: number;
    eventsExported: number;
    truncated: boolean;
  }[];
  actions: { at: string; actor: string; action: string; detail: Record<string, unknown> }[];
  stewards: PublicSceneSteward[];
  transfers: Omit<SceneTransferRow, "sceneId">[];
  migrations: string[];
}

export interface ContinuitySummary {
  changelog: number;
  /** Real bot runs — sandbox sims excluded, per the arena-row rule. */
  reports: number;
  /** Every stored report on the scene, sandbox sims included — the bundle's count. */
  reportsAll: number;
  episodes: number;
  docs: number;
  stewards: number;
}
