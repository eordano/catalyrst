import { z } from "zod";

import { signedFetch } from "../../auth/signer";
import type { AuthIdentity } from "../../auth/types";

export const GOVERNANCE_PROPOSAL_KINDS = [
  "catalyst",
  "hiring",
  "tender",
  "bid",
  "linked-wearables",
  "governance",
  "council-decision-veto",
] as const;

export type GovernanceProposalKind = (typeof GOVERNANCE_PROPOSAL_KINDS)[number];

export const GOVERNANCE_SUBMIT_MOUNT = "/api/governance/proposals";

export function isGovernanceProposalKind(
  raw: string | null | undefined,
): raw is GovernanceProposalKind {
  return GOVERNANCE_PROPOSAL_KINDS.includes(
    (raw ?? "") as GovernanceProposalKind,
  );
}

export function governanceSubmitPath(kind: GovernanceProposalKind): string {
  return `/proposals/${kind}`;
}

export function governanceSubmitUrl(kind: GovernanceProposalKind): string {
  return `${GOVERNANCE_SUBMIT_MOUNT}/${kind}`;
}

const CreatedProposalSchema = z.object({
  id: z.string().min(1),
  type: z.string().nullish(),
  pending: z.boolean().nullish(),
  published: z.boolean().nullish(),
});

export type CreatedProposalRow = z.infer<typeof CreatedProposalSchema>;

const NOT_CONFIGURED_RE =
  /not[\s_-]?configured|is[\s_-]?unset|signer[\s_-]?unavailable|snapshot[\s_-]?private[\s_-]?key/i;

export const SIGN_IN_REQUIRED =
  "Sign in with your Decentraland wallet to submit this proposal.";

export class GovernanceSubmitUnavailableError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "GovernanceSubmitUnavailableError";
  }
}

function serverMessage(text: string): string {
  try {
    const body = JSON.parse(text) as { error?: unknown; message?: unknown };
    const message = typeof body.message === "string" ? body.message.trim() : "";
    const error = typeof body.error === "string" ? body.error.trim() : "";
    return message || error;
  } catch {
    return text.trim();
  }
}

function reportsSignerUnavailable(status: number, message: string): boolean {
  return status === 503 || NOT_CONFIGURED_RE.test(message);
}

export type SubmitProposalArgs = {
  identity: AuthIdentity | null;
  kind: GovernanceProposalKind;
  body: unknown;
  unavailable: string;
  signal?: AbortSignal;
};

export async function submitProposal({
  identity,
  kind,
  body,
  unavailable,
  signal,
}: SubmitProposalArgs): Promise<CreatedProposalRow> {
  if (!identity) throw new Error(SIGN_IN_REQUIRED);

  const res = await signedFetch(identity, governanceSubmitUrl(kind), {
    method: "POST",
    signPath: governanceSubmitPath(kind),
    metadata: {},
    headers: { accept: "application/json", "content-type": "application/json" },
    body: JSON.stringify(body),
    signal,
  });

  const text = await res.text();

  if (!res.ok) {
    const message = serverMessage(text);
    if (reportsSignerUnavailable(res.status, message)) {
      throw new GovernanceSubmitUnavailableError(unavailable);
    }
    throw new Error(message || `Governance submit returned ${res.status}.`);
  }

  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    throw new Error("Governance submit returned a non-JSON response.");
  }

  const enveloped = z.object({ data: CreatedProposalSchema }).safeParse(raw);
  if (enveloped.success) return enveloped.data.data;

  const bare = CreatedProposalSchema.safeParse(raw);
  if (bare.success) return bare.data;

  throw new Error("Governance submit returned no proposal id.");
}
