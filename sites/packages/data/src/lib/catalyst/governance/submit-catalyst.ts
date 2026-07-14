import { z } from "zod";

import fixture from "../../../fixtures/governance-submit-catalyst.json";
import { submitProposal } from "./submit-client";
import type { AuthIdentity } from "../../auth/types";
import { ETH_ADDRESS_RE } from "../format/address";

export const CATALYST_REQUESTS = ["add", "remove"] as const;
export type CatalystRequest = (typeof CATALYST_REQUESTS)[number];

export const CATALYST_TYPE: Record<CatalystRequest, string> = {
  add: "catalyst_add",
  remove: "catalyst_remove",
};

export function toCatalystRequest(
  raw: string | null | undefined,
): CatalystRequest | null {
  const v = raw?.trim().toLowerCase();
  return v === "add" || v === "remove" ? v : null;
}

const RequestCopySchema = z.object({
  request: z.enum(CATALYST_REQUESTS),
  type: z.string(),
  title: z.string(),
  description: z.string(),
  description_2: z.string(),
  description_detail: z.string(),
  submit_label: z.string(),
});

const FieldSchema = z.object({
  label: z.string(),
  placeholder: z.string().nullish(),
  format: z.string().nullish(),
  required: z.boolean().optional(),
  markdown: z.boolean().optional(),
  optional: z.boolean().optional(),
  min_length: z.number().nullish(),
  max_length: z.number().nullish(),
  max: z.number().nullish(),
  address_length: z.number().nullish(),
  help: z.string().nullish(),
});

const SampleNodeSchema = z.object({
  domain: z.string(),
  owner: z.string(),
  label: z.string(),
});

const SubmitCatalystSchema = z.object({
  request_param: z.string(),
  valid_requests: z.array(z.enum(CATALYST_REQUESTS)),
  requests: z.object({
    add: RequestCopySchema,
    remove: RequestCopySchema,
  }),
  fields: z.object({
    owner: FieldSchema,
    domain: FieldSchema,
    description: FieldSchema,
    coAuthors: FieldSchema,
  }),
  domain_status: z.object({
    content_ok: z.string(),
    lambdas_ok: z.string(),
    content_label: z.string(),
    lambdas_label: z.string(),
    invalid: z.string(),
    already_a_catalyst: z.string(),
    not_a_catalyst: z.string(),
  }),
  sample_nodes: z.array(SampleNodeSchema),
  submit_error: z.string(),
  success: z.object({
    title: z.string(),
    lead: z.string(),
    note: z.string(),
  }),
});

export type RequestCopy = z.infer<typeof RequestCopySchema>;
export type SubmitCatalystData = z.infer<typeof SubmitCatalystSchema>;

const FALLBACK: SubmitCatalystData = {
  request_param: "request",
  valid_requests: [...CATALYST_REQUESTS],
  requests: {
    add: {
      request: "add",
      type: "catalyst_add",
      title: "Add a catalyst node",
      description:
        "Decentraland is run on a network of community-operated nodes that store scenes and route peer connections.",
      description_2: "To propose the addition of a new node, please provide the following details.",
      description_detail: "Explain why this node should be added to the Catalyst network.",
      submit_label: "Submit proposal",
    },
    remove: {
      request: "remove",
      type: "catalyst_remove",
      title: "Remove a catalyst node",
      description:
        "Decentraland is run on a network of community-operated nodes that store scenes and route peer connections.",
      description_2: "To propose the removal of a node, please provide the following details.",
      description_detail: "Explain why this node should be removed from the Catalyst network.",
      submit_label: "Submit proposal",
    },
  },
  fields: {
    owner: {
      label: "Ethereum address of the owner of the Catalyst Node",
      placeholder: "Example: 0x06012c8cf97bead5deae237070f9587f8e7a266d",
      format: "address",
      required: true,
      markdown: false,
      optional: false,
    },
    domain: {
      label: "Domain for the Catalyst Node",
      placeholder: "Example: catalyst.yourdomainname.com",
      format: "hostname",
      required: true,
      markdown: false,
      optional: false,
    },
    description: {
      label: "Description",
      placeholder: "Write your description using markdown...",
      required: true,
      markdown: true,
      optional: false,
      min_length: 20,
      max_length: 250,
    },
    coAuthors: {
      label: "Co-authors",
      required: false,
      markdown: false,
      optional: true,
      max: 5,
      address_length: 42,
    },
  },
  domain_status: {
    content_ok: "Content server is ready.",
    lambdas_ok: "Lambda server is ready.",
    content_label: "Content server status",
    lambdas_label: "Lambda server status",
    invalid: "Couldn't get the status of the servers running on the provided domain.",
    already_a_catalyst: "This domain is already part of the Catalyst network.",
    not_a_catalyst: "This domain is not part of the Catalyst network.",
  },
  sample_nodes: [],
  submit_error: "There was an error while trying to create the proposal, please try again later.",
  success: {
    title: "Proposal created",
    lead: "Your Catalyst node proposal is now live for the DAO to vote on.",
    note: "This proposal was created in a simulation \u{2014} no on-chain transaction or Snapshot proposal was submitted.",
  },
};

function parse(): SubmitCatalystData {
  const parsed = SubmitCatalystSchema.safeParse(fixture);
  return parsed.success ? parsed.data : FALLBACK;
}

export function getSubmitCatalystData(): SubmitCatalystData {
  return parse();
}

export function isValidDomainName(domain: string): boolean {
  return /^(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z0-9][a-z0-9-]{0,61}[a-z0-9]$/.test(
    domain.trim().toLowerCase(),
  );
}

export function isEthAddress(addr: string): boolean {
  return ETH_ADDRESS_RE.test(addr.trim());
}

export type DomainStatus = {
  domain: string;
  valid: boolean;
  contentOk: boolean;
  lambdasOk: boolean;
  alreadyACatalyst: boolean;
};

export function simulateDomainStatus(domain: string): DomainStatus {
  const d = domain.trim().toLowerCase().replace(/^https?:\/\//, "");
  const valid = isValidDomainName(d);
  const known = parse().sample_nodes.some((n) => n.domain.toLowerCase() === d);
  return {
    domain: d,
    valid,
    contentOk: valid,
    lambdasOk: valid,
    alreadyACatalyst: known,
  };
}

export type NewProposalCatalyst = {
  request: CatalystRequest;
  type: string;
  owner: string;
  domain: string;
  description: string;
  coAuthors: string[];
};

export type CreatedProposal = {
  id: string;
  type: string;
  request: CatalystRequest;
};

export function buildProposalPayload(
  request: CatalystRequest,
  input: { owner: string; domain: string; description: string; coAuthors: string[] },
): NewProposalCatalyst {
  return {
    request,
    type: CATALYST_TYPE[request],
    owner: input.owner.trim(),
    domain: input.domain.trim().toLowerCase().replace(/^https?:\/\//, ""),
    description: input.description.trim(),
    coAuthors: input.coAuthors.map((a) => a.trim()).filter(Boolean),
  };
}

export type CreateProposalFn = (args: {
  payload: NewProposalCatalyst;
  signal?: AbortSignal;
}) => Promise<CreatedProposal>;

const SUBMIT_UNAVAILABLE =
  "catalyst proposal submission unavailable: DAO governance signer not configured";

export const failClosedCreateProposal: CreateProposalFn = async () => {
  throw new Error(SUBMIT_UNAVAILABLE);
};

export function buildCreateProposal(
  identity: AuthIdentity | null,
): CreateProposalFn {
  return async ({ payload, signal }) => {
    const created = await submitProposal({
      identity,
      kind: "catalyst",
      body: payload,
      unavailable: SUBMIT_UNAVAILABLE,
      signal,
    });
    return {
      id: created.id,
      type: created.type ?? payload.type,
      request: payload.request,
    };
  };
}
