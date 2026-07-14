import { z } from "zod";

import fixture from "../../../fixtures/governance-edit-project-update.json";
import { warnInvalid } from "../warn";
import { governanceApiBase } from "./api-base";

export { governanceApiBase };

export type ProjectHealth = "onTrack" | "atRisk" | "offTrack";

export type FinancialRecord = {
  category: string;
  description: string;
  token: string;
  amount: number;
  receiver: string;
  link?: string;
};

export type ProjectUpdate = {
  id: string;
  proposalId: string;
  projectId: string;
  health: ProjectHealth;
  introduction: string;
  highlights: string;
  blockers: string;
  next_steps: string;
  additional_notes: string;
  status: string;
  financial_records: FinancialRecord[];
};

export type ProjectSummary = {
  id: string;
  proposalId: string;
  title: string;
  type: string;
  category: string;
};

export type EditUpdateData = {
  /**
   * "live" -- the update below is a record this node holds.
   * "unavailable" -- there is nothing to edit here; `reason` says why. Callers
   *   must render the reason, never an editable form full of fixture text.
   * "fixture" -- only produced by `fixtureEditUpdate()`, for stories and layout
   *   work. `loadEditUpdate` never returns it.
   */
  source: "live" | "fixture" | "unavailable";
  /** Set when source === "unavailable". Safe to show to a visitor. */
  reason?: string;
  project: ProjectSummary;
  update: ProjectUpdate;
  fundsReleasedSinceLastUpdate: number;
  fundsReleasedTxCount: number;
  fundsReleasedLastTxDate: string;
};

type FixtureShape = {
  _source: string;
  project: {
    id: string;
    proposal_id: string;
    title: string;
    type: string;
    category: string;
  };
  update: {
    id: string;
    proposal_id: string;
    project_id: string;
    health: ProjectHealth;
    introduction: string;
    highlights: string;
    blockers: string;
    next_steps: string;
    additional_notes: string;
    status: string;
    financial_records: FinancialRecord[];
  };
  fundsReleasedSinceLastUpdate: number;
  fundsReleasedTxCount: number;
  fundsReleasedLastTxDate: string;
};

const FIXTURE = fixture as unknown as FixtureShape;

function fixtureUpdate(): ProjectUpdate {
  const u = FIXTURE.update;
  return {
    id: u.id,
    proposalId: u.proposal_id,
    projectId: u.project_id,
    health: u.health,
    introduction: u.introduction,
    highlights: u.highlights,
    blockers: u.blockers,
    next_steps: u.next_steps,
    additional_notes: u.additional_notes ?? "",
    status: u.status,
    financial_records: u.financial_records ?? [],
  };
}

const EMPTY_UPDATE: ProjectUpdate = {
  id: "",
  proposalId: "",
  projectId: "",
  health: "onTrack",
  introduction: "",
  highlights: "",
  blockers: "",
  next_steps: "",
  additional_notes: "",
  status: "",
  financial_records: [],
};

export function unavailableEditUpdate(reason: string): EditUpdateData {
  return {
    source: "unavailable",
    reason,
    project: { id: "", proposalId: "", title: "", type: "", category: "" },
    update: EMPTY_UPDATE,
    fundsReleasedSinceLastUpdate: 0,
    fundsReleasedTxCount: 0,
    fundsReleasedLastTxDate: "",
  };
}

export function fixtureEditUpdate(): EditUpdateData {
  return {
    source: "fixture",
    project: {
      id: FIXTURE.project.id,
      proposalId: FIXTURE.project.proposal_id,
      title: FIXTURE.project.title,
      type: FIXTURE.project.type,
      category: FIXTURE.project.category,
    },
    update: fixtureUpdate(),
    fundsReleasedSinceLastUpdate: FIXTURE.fundsReleasedSinceLastUpdate,
    fundsReleasedTxCount: FIXTURE.fundsReleasedTxCount,
    fundsReleasedLastTxDate: FIXTURE.fundsReleasedLastTxDate,
  };
}

const HEALTHS: ProjectHealth[] = ["onTrack", "atRisk", "offTrack"];

const FinancialRecordSchema = z.object({
  category: z.string(),
  description: z.string(),
  token: z.string(),
  amount: z.number(),
  receiver: z.string(),
  link: z.string().optional().nullable(),
});

const UpdateSchema = z.object({
  id: z.string(),
  proposal_id: z.string(),
  project_id: z.string().optional().nullable(),
  health: z.string().nullable().optional(),
  introduction: z.string().nullable().optional(),
  highlights: z.string().nullable().optional(),
  blockers: z.string().nullable().optional(),
  next_steps: z.string().nullable().optional(),
  additional_notes: z.string().nullable().optional(),
  status: z.string().optional().nullable(),
  financial_records: z.array(FinancialRecordSchema).nullable().optional(),
  created_at: z.string().optional().nullable(),
  updated_at: z.string().optional().nullable(),
});

type RawUpdate = z.infer<typeof UpdateSchema>;

function normalizeHealth(raw: string | null | undefined): ProjectHealth {
  return HEALTHS.includes(raw as ProjectHealth)
    ? (raw as ProjectHealth)
    : "onTrack";
}

function hasContent(u: RawUpdate): boolean {
  return Boolean(u.introduction && u.introduction.trim());
}

function projectUpdate(u: RawUpdate, projectId: string): ProjectUpdate {
  return {
    id: u.id,
    proposalId: u.proposal_id,
    projectId: u.project_id ?? projectId,
    health: normalizeHealth(u.health),
    introduction: u.introduction ?? "",
    highlights: u.highlights ?? "",
    blockers: u.blockers ?? "",
    next_steps: u.next_steps ?? "",
    additional_notes: u.additional_notes ?? "",
    status: u.status ?? "done",
    financial_records:
      u.financial_records && u.financial_records.length > 0
        ? u.financial_records.map((r) => ({
            category: r.category,
            description: r.description,
            token: r.token,
            amount: r.amount,
            receiver: r.receiver,
            link: r.link ?? undefined,
          }))
        : [],
  };
}

const VestingLogSchema = z
  .object({
    timestamp: z.string().optional().nullable(),
    amount: z.number().optional().nullable(),
  })
  .passthrough();

const ProjectMetaSchema = z
  .object({
    id: z.string(),
    proposal_id: z.string().optional().nullable(),
    title: z.string(),
    type: z.string().optional().nullable(),
    configuration: z
      .object({ category: z.string().optional().nullable() })
      .passthrough()
      .optional()
      .nullable(),
    funding: z
      .object({
        vesting: z
          .object({ logs: z.array(VestingLogSchema).optional().nullable() })
          .passthrough()
          .optional()
          .nullable(),
      })
      .passthrough()
      .optional()
      .nullable(),
  })
  .passthrough();

const ProjectsMetaResponseSchema = z.object({
  data: z.array(ProjectMetaSchema),
});

/**
 * Updates arrive nested inside GET /projects/{id}
 * (catalyrst-governance/src/handlers/read.rs:201 -> parse.rs:151). There is
 * no standalone GET /updates?project_id= route on this node -- lib.rs:29-44
 * registers none -- so the edit form must not fall back to fixture text on
 * a failed read.
 */
const ProjectDetailResponseSchema = ProjectMetaSchema.extend({
  updates: z.array(UpdateSchema).nullish(),
});

type ProjectMeta = z.infer<typeof ProjectMetaSchema>;

function fundsReleasedSince(
  project: ProjectMeta,
  boundaryIso: string | null | undefined,
): { amount: number; txCount: number; lastDate: string } {
  const logs = project.funding?.vesting?.logs ?? [];
  const boundary = boundaryIso ? Date.parse(boundaryIso) : NaN;
  const since = logs.filter(
    (l) =>
      l.timestamp &&
      (Number.isNaN(boundary) || Date.parse(l.timestamp) >= boundary),
  );
  const amount = since.reduce((sum, l) => sum + (l.amount ?? 0), 0);
  const lastDate =
    since
      .map((l) => l.timestamp)
      .filter((t): t is string => !!t)
      .sort()
      .pop() ?? "";
  return { amount, txCount: since.length, lastDate };
}

export type LoadEditUpdateOptions = {
  base?: string;
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
};

export async function loadEditUpdate(
  projectId: string,
  opts: LoadEditUpdateOptions = {},
): Promise<EditUpdateData> {
  const id = projectId?.trim();
  if (!id) return unavailableEditUpdate("no project id in the URL");

  const base = governanceApiBase(opts.base);
  const url = `${base}/projects/${encodeURIComponent(id)}`;
  const doFetch = opts.fetchImpl ?? fetch;

  try {
    const res = await doFetch(url, {
      headers: { accept: "application/json" },
      signal: opts.signal,
    });
    if (res.status === 404) {
      return unavailableEditUpdate("this node has no project with that id");
    }
    if (!res.ok) {
      return unavailableEditUpdate(
        `governance projects endpoint returned ${res.status}`,
      );
    }
    const parsed = ProjectDetailResponseSchema.safeParse(
      (await res.json()) as unknown,
    );
    if (!parsed.success) {
      warnInvalid("governance /projects/{id}", parsed.error?.issues);
      return unavailableEditUpdate(
        "governance projects endpoint returned an unrecognised payload",
      );
    }
    const meta = parsed.data;

    const candidate = (meta.updates ?? [])
      .filter(hasContent)
      .sort((a, b) => (b.created_at ?? "").localeCompare(a.created_at ?? ""))[0];
    if (!candidate) {
      return unavailableEditUpdate(
        "this project has no published update to edit",
      );
    }

    const funds = fundsReleasedSince(meta, candidate.created_at);
    return {
      source: "live",
      project: {
        id,
        proposalId: meta.proposal_id ?? candidate.proposal_id,
        title: meta.title,
        type: meta.type ?? "",
        category: meta.configuration?.category ?? "",
      },
      update: projectUpdate(candidate, id),
      fundsReleasedSinceLastUpdate: funds.amount,
      fundsReleasedTxCount: funds.txCount,
      fundsReleasedLastTxDate: funds.lastDate,
    };
  } catch (err) {
    return unavailableEditUpdate(
      `governance projects endpoint unreachable: ${
        err instanceof Error ? err.message : String(err)
      }`,
    );
  }
}
