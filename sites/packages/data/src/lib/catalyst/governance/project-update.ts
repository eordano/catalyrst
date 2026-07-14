import { z } from "zod";

import fixture from "../../../fixtures/governance-submit-project-update.json";
import { warnInvalid } from "../warn";
import type { ListEnvelope as RsListEnvelope } from "@ui/generated/catalyst/governance/ListEnvelope";
import { governanceApiBase } from "./api-base";

export { governanceApiBase };

export type HealthOption = { id: string; label: string };

export type UpdateField = {
  key: string;
  label: string;
  limit: number;
  required: boolean;
};

export type ProjectFunding = {
  token: string;
  total: number;
  totalLabel: string;
  vested: number;
  vestedLabel: string;
  released: number;
  releasedLabel: string;
  releasable: number;
  releasableLabel: string;
  releasedSinceLastUpdate: number;
  releasedSinceLastUpdateLabel: string;
  txCountSinceLastUpdate: number;
  vestingStartAt: string | null;
  vestingFinishAt: string | null;
  address: string | null;
};

export type UpdateProject = {
  id: string;
  proposalId: string;
  title: string;
  type: string;
  status: string;
  category: string | null;
  tier: string | null;
  size: number | null;
  author: string | null;
};

export type PriorUpdate = {
  id: string;
  health: string | null;
  introduction: string;
  highlights: string;
  blockers: string;
  next_steps: string;
  status: string | null;
  completion_date: string | null;
};

export type ProjectUpdateContext = {
  /**
   * "live" -- the project and its prior updates came from this node.
   * "unavailable" -- nothing below describes a real project; `reason` says why.
   * "fixture" -- only produced by `fixtureContext()`, for stories and layout
   *   work. `loadProjectUpdateContext` never returns it.
   */
  source: "live" | "fixture" | "unavailable";
  /** Set when source === "unavailable". Safe to show to a visitor. */
  reason?: string;
  project: UpdateProject;
  funding: ProjectFunding;
  healthOptions: HealthOption[];
  fields: UpdateField[];
  csvHeader: string;
  recordCategories: string[];
  priorUpdates: PriorUpdate[];
};

type Fixture = {
  project: UpdateProject & {
    proposal_id?: string;
  };
  funding: ProjectFunding;
  generalSection: { healthOptions: HealthOption[]; fields: UpdateField[] };
  financialSection: { csvHeader: string; recordCategories: string[] };
  priorUpdates: PriorUpdate[];
};

const FIXTURE = fixture as unknown as Fixture;

function fixtureProject(): UpdateProject {
  const p = FIXTURE.project as UpdateProject & { proposal_id?: string };
  return {
    id: p.id,
    proposalId: p.proposalId ?? p.proposal_id ?? "",
    title: p.title,
    type: p.type,
    status: p.status,
    category: p.category ?? null,
    tier: p.tier ?? null,
    size: p.size ?? null,
    author: p.author ?? null,
  };
}

export function fixtureContext(): ProjectUpdateContext {
  return {
    source: "fixture",
    project: fixtureProject(),
    funding: FIXTURE.funding,
    healthOptions: FIXTURE.generalSection.healthOptions,
    fields: FIXTURE.generalSection.fields,
    csvHeader: FIXTURE.financialSection.csvHeader,
    recordCategories: FIXTURE.financialSection.recordCategories,
    priorUpdates: FIXTURE.priorUpdates,
  };
}

const EMPTY_PROJECT: UpdateProject = {
  id: "",
  proposalId: "",
  title: "",
  type: "",
  status: "",
  category: null,
  tier: null,
  size: null,
  author: null,
};

const EMPTY_FUNDING: ProjectFunding = {
  token: "",
  total: 0,
  totalLabel: "",
  vested: 0,
  vestedLabel: "",
  released: 0,
  releasedLabel: "",
  releasable: 0,
  releasableLabel: "",
  releasedSinceLastUpdate: 0,
  releasedSinceLastUpdateLabel: "",
  txCountSinceLastUpdate: 0,
  vestingStartAt: null,
  vestingFinishAt: null,
  address: null,
};

/**
 * The form scaffolding (health options, field list, CSV header, record
 * categories) is static copy, not data -- it is safe to keep from the fixture.
 * The project, its funding and its prior updates are measurements and are
 * blanked: the caller must render `reason`, never these zeros.
 */
export function unavailableContext(reason: string): ProjectUpdateContext {
  return {
    source: "unavailable",
    reason,
    project: EMPTY_PROJECT,
    funding: EMPTY_FUNDING,
    healthOptions: FIXTURE.generalSection.healthOptions,
    fields: FIXTURE.generalSection.fields,
    csvHeader: FIXTURE.financialSection.csvHeader,
    recordCategories: FIXTURE.financialSection.recordCategories,
    priorUpdates: [],
  };
}

const VestingSchema = z
  .object({
    token: z.string().optional(),
    total: z.number().optional(),
    vested: z.number().optional(),
    released: z.number().optional(),
    releasable: z.number().optional(),
    address: z.string().nullable().optional(),
    vestingStartAt: z.string().nullable().optional(),
    vestingFinishAt: z.string().nullable().optional(),
    start_at: z.string().nullable().optional(),
    finish_at: z.string().nullable().optional(),
    logs: z.array(z.unknown()).optional(),
  })
  .passthrough();

const ProjectSchema = z
  .object({
    id: z.string(),
    proposal_id: z.string(),
    title: z.string(),
    type: z.string(),
    status: z.string(),
    author: z.string().nullable().optional(),
    configuration: z
      .object({
        category: z.string().nullable().optional(),
        tier: z.string().nullable().optional(),
        size: z.number().nullable().optional(),
      })
      .passthrough()
      .optional(),
    funding: z
      .object({ vesting: VestingSchema.nullable().optional() })
      .passthrough()
      .nullable()
      .optional(),
  })
  .passthrough();

const ProjectsResponseSchema = z.object({ data: z.array(ProjectSchema) });

const UpdateRecordSchema = z
  .object({
    id: z.string(),
    health: z.string().nullable().optional(),
    introduction: z.string().nullable().optional(),
    highlights: z.string().nullable().optional(),
    blockers: z.string().nullable().optional(),
    next_steps: z.string().nullable().optional(),
    status: z.string().nullable().optional(),
    completion_date: z.string().nullable().optional(),
  })
  .passthrough();

/**
 * Project updates come nested inside GET /projects/{id}
 * (catalyrst-governance/src/handlers/read.rs:201 -> parse.rs:151, which inserts
 * the stored update rows under "updates"). There is no standalone /updates
 * route on this node -- lib.rs:29-44 does not register one.
 */
const ProjectDetailResponseSchema = ProjectSchema.extend({
  updates: z.array(UpdateRecordSchema).nullish(),
});

type LiveProject = z.infer<typeof ProjectSchema>;

function money(n: number): string {
  return "$" + Math.round(n).toLocaleString("en-US");
}

function projectFunding(p: LiveProject): ProjectFunding {
  const v = p.funding?.vesting ?? {};
  const total = v.total ?? 0;
  const vested = v.vested ?? 0;
  const released = v.released ?? 0;
  const releasable = v.releasable ?? 0;
  const releasedSince = released;
  return {
    token: v.token ?? "USDC",
    total,
    totalLabel: money(total),
    vested,
    vestedLabel: money(vested),
    released,
    releasedLabel: money(released),
    releasable,
    releasableLabel: money(releasable),
    releasedSinceLastUpdate: releasedSince,
    releasedSinceLastUpdateLabel: money(releasedSince),
    txCountSinceLastUpdate: (v.logs ?? []).length,
    vestingStartAt: v.vestingStartAt ?? v.start_at ?? null,
    vestingFinishAt: v.vestingFinishAt ?? v.finish_at ?? null,
    address: v.address ?? null,
  };
}

function projectVM(p: LiveProject): UpdateProject {
  return {
    id: p.id,
    proposalId: p.proposal_id,
    title: p.title,
    type: p.type,
    status: p.status,
    category: p.configuration?.category ?? null,
    tier: p.configuration?.tier ?? null,
    size: p.configuration?.size ?? null,
    author: p.author ?? null,
  };
}

export type LoadOptions = {
  base?: string;
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
};

export async function loadProjectUpdateContext(
  projectId: string | undefined,
  opts: LoadOptions = {},
): Promise<ProjectUpdateContext> {
  // Only the static form copy is taken from the fixture; every measurement
  // below comes from the node or is reported as unavailable.
  const fb = fixtureContext();
  if (!projectId) return unavailableContext("no project id in the URL");

  const base = governanceApiBase(opts.base);
  const doFetch = opts.fetchImpl ?? fetch;

  try {
    const res = await doFetch(
      `${base}/projects/${encodeURIComponent(projectId)}`,
      { headers: { accept: "application/json" }, signal: opts.signal },
    );
    if (res.status === 404) {
      return unavailableContext("this node has no project with that id");
    }
    if (!res.ok) {
      return unavailableContext(`governance projects endpoint returned ${res.status}`);
    }
    const parsed = ProjectDetailResponseSchema.safeParse(
      (await res.json()) as unknown,
    );
    if (!parsed.success) {
      warnInvalid("governance /projects/{id}", parsed.error?.issues);
      return unavailableContext(
        "governance projects endpoint returned an unrecognised payload",
      );
    }
    const live = parsed.data;

    const priorUpdates: PriorUpdate[] = (live.updates ?? [])
      .filter((u) => u.health)
      .slice(-3)
      .reverse()
      .map((u) => ({
        id: u.id,
        health: u.health ?? null,
        introduction: (u.introduction ?? "").slice(0, 600),
        highlights: (u.highlights ?? "").slice(0, 600),
        blockers: (u.blockers ?? "").slice(0, 400),
        next_steps: (u.next_steps ?? "").slice(0, 400),
        status: u.status ?? null,
        completion_date: u.completion_date ?? null,
      }));

    return {
      source: "live",
      project: projectVM(live),
      funding: projectFunding(live),
      healthOptions: fb.healthOptions,
      fields: fb.fields,
      csvHeader: fb.csvHeader,
      recordCategories: fb.recordCategories,
      priorUpdates,
    };
  } catch (err) {
    return unavailableContext(
      `governance projects endpoint unreachable: ${
        err instanceof Error ? err.message : String(err)
      }`,
    );
  }
}

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Assert<T extends true> = T;
export type _DriftProjectsEnvelope = Assert<
  AssignableTo<
    RsListEnvelope,
    Omit<z.input<typeof ProjectsResponseSchema>, "data"> & {
      data: RsListEnvelope["data"];
    }
  >
>;
