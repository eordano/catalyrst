import { z } from "zod";

import { warnInvalid } from "../warn";
import { governanceApiBase } from "./api-base";

export type ProjectHealth = "onTrack" | "atRisk" | "offTrack";

export type FinancialRecord = {
  category: string;
  description: string;
  token: string;
  amount: number;
  receiver: string;
  link?: string;
};

type ProjectUpdate = {
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

type ProjectSummary = {
  id: string;
  proposalId: string;
  title: string;
  type: string;
  category: string;
};

export type EditUpdateData = {
  source: "live" | "fixture" | "unavailable";
  reason?: string;
  project: ProjectSummary;
  update: ProjectUpdate;
  fundsReleasedSinceLastUpdate: number;
  fundsReleasedTxCount: number;
  fundsReleasedLastTxDate: string;
};

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

function unavailableEditUpdate(reason: string): EditUpdateData {
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

z.object({
  data: z.array(ProjectMetaSchema),
});

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

type LoadEditUpdateOptions = {
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
