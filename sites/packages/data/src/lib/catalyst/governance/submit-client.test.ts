import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../auth/signer", () => ({ signedFetch: vi.fn() }));

import { signedFetch } from "../../auth/signer";
import {
  GovernanceSubmitUnavailableError,
  governanceSubmitPath,
  governanceSubmitUrl,
  submitProposal,
} from "./submit-client";
import { buildCreateProposal } from "./submit-catalyst";
import { buildCreateTender } from "./submit-tender";
import { buildSubmitBid } from "./submit-bid";
import type { AuthIdentity } from "../../auth/types";

const mSignedFetch = vi.mocked(signedFetch);

const IDENTITY: AuthIdentity = {
  signer: "0x4e9c4a2502fdf71e93ed8ed6ca9ddbd891d6f295",
  ephemeral: { address: "0xeph", privateKey: "0xdeadbeef" },
  expiration: "2999-01-01T00:00:00.000Z",
  authChain: [],
};

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

const CATALYST_PAYLOAD = {
  request: "add" as const,
  type: "catalyst_add",
  owner: "0x06012c8cf97bead5deae237070f9587f8e7a266d",
  domain: "catalyst.example.org",
  description: "A new node for the network.",
  coAuthors: [],
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe("submitProposal \u{2014} governance write seam", () => {
  it("signs the upstream proposal path while posting to the same-origin mount", async () => {
    mSignedFetch.mockResolvedValue(jsonResponse(201, { id: "prop-1", type: "catalyst_add" }));

    const created = await submitProposal({
      identity: IDENTITY,
      kind: "catalyst",
      body: CATALYST_PAYLOAD,
      unavailable: "nope",
    });

    expect(created.id).toBe("prop-1");
    const [identity, url, init] = mSignedFetch.mock.calls[0];
    expect(identity).toBe(IDENTITY);
    expect(url).toBe(governanceSubmitUrl("catalyst"));
    expect(url).toBe("/api/governance/proposals/catalyst");
    const typed = init as { method: string; signPath: string; body: string };
    expect(typed.method).toBe("POST");
    expect(typed.signPath).toBe(governanceSubmitPath("catalyst"));
    expect(JSON.parse(typed.body)).toEqual(CATALYST_PAYLOAD);
  });

  it("accepts a data-enveloped response", async () => {
    mSignedFetch.mockResolvedValue(jsonResponse(200, { data: { id: "prop-2" } }));

    const created = await submitProposal({
      identity: IDENTITY,
      kind: "hiring",
      body: {},
      unavailable: "nope",
    });

    expect(created.id).toBe("prop-2");
  });

  it("fails closed without an identity and never calls the network", async () => {
    await expect(
      submitProposal({
        identity: null,
        kind: "catalyst",
        body: CATALYST_PAYLOAD,
        unavailable: "nope",
      }),
    ).rejects.toThrow(/sign in/i);
    expect(mSignedFetch).not.toHaveBeenCalled();
  });

  it("maps a 503 not-configured backend to the fail-closed message", async () => {
    mSignedFetch.mockResolvedValue(
      jsonResponse(503, { ok: false, error: "not configured", message: "signer missing" }),
    );

    await expect(
      submitProposal({
        identity: IDENTITY,
        kind: "catalyst",
        body: CATALYST_PAYLOAD,
        unavailable: "catalyst proposal submission unavailable: DAO governance signer not configured",
      }),
    ).rejects.toThrow(GovernanceSubmitUnavailableError);
  });

  it("maps a 500 whose message names the missing key to the fail-closed message", async () => {
    mSignedFetch.mockResolvedValue(
      jsonResponse(500, { ok: false, message: "SNAPSHOT_PRIVATE_KEY is not set" }),
    );

    await expect(
      submitProposal({
        identity: IDENTITY,
        kind: "tender",
        body: {},
        unavailable: "tender unavailable",
      }),
    ).rejects.toThrow("tender unavailable");
  });

  it("surfaces a 501 verbatim rather than blaming a missing signing key", async () => {
    mSignedFetch.mockResolvedValue(
      jsonResponse(501, {
        error:
          "bid submission is not implemented: a bid does not create a snapshot proposal when it is submitted",
      }),
    );

    await expect(
      submitProposal({
        identity: IDENTITY,
        kind: "bid",
        body: {},
        unavailable: "bid unavailable",
      }),
    ).rejects.toThrow(/does not create a snapshot proposal/);
  });

  it("surfaces other backend errors verbatim rather than swallowing them", async () => {
    mSignedFetch.mockResolvedValue(
      jsonResponse(400, { ok: false, message: "linked_proposal_id is required" }),
    );

    await expect(
      submitProposal({
        identity: IDENTITY,
        kind: "bid",
        body: {},
        unavailable: "bid unavailable",
      }),
    ).rejects.toThrow("linked_proposal_id is required");
  });

  it("refuses to invent an id when the backend answers 200 with no id", async () => {
    mSignedFetch.mockResolvedValue(jsonResponse(200, { ok: true }));

    await expect(
      submitProposal({
        identity: IDENTITY,
        kind: "governance",
        body: {},
        unavailable: "governance unavailable",
      }),
    ).rejects.toThrow(/no proposal id/i);
  });
});

describe("per-wizard factories", () => {
  it("catalyst returns the server id and echoes the submitted request", async () => {
    mSignedFetch.mockResolvedValue(jsonResponse(201, { id: "cat-9" }));

    const created = await buildCreateProposal(IDENTITY)({ payload: CATALYST_PAYLOAD });

    expect(created).toEqual({ id: "cat-9", type: "catalyst_add", request: "add" });
  });

  it("tender posts the trimmed form and reports the server pending flag", async () => {
    mSignedFetch.mockResolvedValue(jsonResponse(201, { id: "tender-3", pending: false }));

    const created = await buildCreateTender(IDENTITY)({
      form: {
        linked_proposal_id: " pitch-1 ",
        project_name: " Name ",
        summary: "s",
        problem_statement: "p",
        technical_specification: "t",
        use_cases: "u",
        deliverables: "d",
        target_release_quarter: "2026 Q4",
        coAuthors: [" ", "0xabc"],
      },
    });

    expect(created).toEqual({
      id: "tender-3",
      type: "tender",
      linked_proposal_id: "pitch-1",
      pending: false,
    });
    const body = JSON.parse((mSignedFetch.mock.calls[0][2] as { body: string }).body);
    expect(body.linked_proposal_id).toBe("pitch-1");
    expect(body.project_name).toBe("Name");
    expect(body.coAuthors).toEqual(["0xabc"]);
  });

  it("bid never claims publication the server did not report", async () => {
    mSignedFetch.mockResolvedValue(jsonResponse(201, { id: "bid-7" }));

    const created = await buildSubmitBid(IDENTITY)({
      tenderId: "tender-3",
      budget: 1000,
      duration: 3,
    });

    expect(created).toEqual({ proposalId: "bid-7", published: false });
  });

  it("bid fails closed when the backend has no signer", async () => {
    mSignedFetch.mockResolvedValue(
      jsonResponse(503, { error: "SNAPSHOT_PRIVATE_KEY not configured" }),
    );

    await expect(
      buildSubmitBid(IDENTITY)({ tenderId: "t", budget: 1, duration: 1 }),
    ).rejects.toThrow(/DAO governance signer not configured/);
  });
});
