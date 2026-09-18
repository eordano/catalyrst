import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../auth/signer", () => ({ signedFetch: vi.fn() }));
vi.mock("../client", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../client")>();
  return { ...actual, postJSON: vi.fn() };
});

import { signedFetch } from "../../auth/signer";
import { postJSON } from "../client";
import { buildSubmitReport } from "./report";
import type { ReportDraft } from "./report";
import type { AuthIdentity } from "../../auth/types";

const mSignedFetch = vi.mocked(signedFetch);
const mPostJSON = vi.mocked(postJSON);

const IDENTITY: AuthIdentity = {
  signer: "0x4e9c4a2502fdf71e93ed8ed6ca9ddbd891d6f295",
  ephemeral: { address: "0xeph", privateKey: "0xdeadbeef" },
  expiration: "2999-01-01T00:00:00.000Z",
  authChain: [],
};

const REPORT_ID = "c11a0899-c86c-4df8-8d46-0a78d99242e0";
const KEY = "0-clip.png";

function draft(overrides: Partial<ReportDraft> = {}): ReportDraft {
  return {
    playerAddress: IDENTITY.signer,
    reportedAddress: "0xCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC",
    reason: "harassment",
    description: "  followed me across three scenes  ",
    evidence: [
      {
        id: "e1",
        name: "clip.png",
        size: 4,
        blob: new Blob(["abcd"], { type: "image/png" }),
        contentType: "image/png",
      },
    ],
    additionalComments: "  ",
    confirmAccuracy: true,
    ...overrides,
  };
}

function presignResponse() {
  return {
    reportId: REPORT_ID,
    files: [
      { key: KEY, uploadPath: `/reports/players/${REPORT_ID}/evidence/${KEY}` },
    ],
  };
}

function createdResponse() {
  return { data: { id: REPORT_ID, evidenceKeys: [KEY] } };
}

function happyPath() {
  mPostJSON
    .mockResolvedValueOnce(presignResponse())
    .mockResolvedValueOnce(createdResponse());
  mSignedFetch.mockResolvedValue(new Response(null, { status: 204 }));
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("buildSubmitReport \u{2014} real comms player-report write", () => {
  it("presigns, uploads the bytes, then creates the report under the server's id, never a locally minted one", async () => {
    happyPath();

    const out = await buildSubmitReport(IDENTITY)({ draft: draft() });

    expect(out).toEqual({ reportId: REPORT_ID, evidenceKeys: [KEY] });
    expect(out.reportId).not.toMatch(/^sim-report-/);

    const [presignPath, presignBody, presignOpts] = mPostJSON.mock.calls[0];
    expect(presignPath).toBe("/comms/reports/players/presign");
    expect((presignOpts as { signPath: string }).signPath).toBe(
      "/reports/players/presign",
    );
    expect(presignBody).toEqual({
      files: [{ filename: "clip.png", contentType: "image/png", fileSize: 4 }],
    });

    expect(mSignedFetch).toHaveBeenCalledTimes(1);
    const [, url, init] = mSignedFetch.mock.calls[0];
    expect(String(url)).toMatch(
      new RegExp(`/comms/reports/players/${REPORT_ID}/evidence/${KEY}$`),
    );
    const put = init as { method: string; signPath: string; body: Blob };
    expect(put.method).toBe("PUT");
    expect(put.signPath).toBe(`/reports/players/${REPORT_ID}/evidence/${KEY}`);
    expect(put.body).toBeInstanceOf(Blob);

    const [createPath, createBody, createOpts] = mPostJSON.mock.calls[1];
    expect(createPath).toBe("/comms/reports/players");
    expect((createOpts as { signPath: string }).signPath).toBe(
      "/reports/players",
    );
    expect(createBody).toEqual({
      reportId: REPORT_ID,
      reportedAddress: "0xcccccccccccccccccccccccccccccccccccccccc",
      reason: "harassment",
      description: "followed me across three scenes",
      additionalComments: undefined,
      confirmAccuracy: true,
      evidenceKeys: [KEY],
    });
  });

  it("never sends a client-chosen reporter \u{2014} the server binds it to the signer", async () => {
    happyPath();

    await buildSubmitReport(IDENTITY)({
      draft: draft({ playerAddress: "0x0000000000000000000000000000000000000bad" }),
    });

    const createBody = mPostJSON.mock.calls[1][1] as Record<string, unknown>;
    expect(createBody).not.toHaveProperty("playerAddress");
    expect(JSON.stringify(createBody)).not.toContain("0bad");
  });

  it("fails closed before any upload: no identity, evidence without bytes, an unconfirmed draft, or a presign plan that skips a file", async () => {
    await expect(
      buildSubmitReport(null)({ draft: draft() }),
    ).rejects.toThrow(/connect your wallet/i);
    await expect(
      buildSubmitReport(IDENTITY)({
        draft: draft({ evidence: [{ id: "e1", name: "clip.png", size: 4 }] }),
      }),
    ).rejects.toThrow(/re-attach your evidence/i);
    await expect(
      buildSubmitReport(IDENTITY)({ draft: draft({ confirmAccuracy: false }) }),
    ).rejects.toThrow(/confirm/i);
    expect(mPostJSON).not.toHaveBeenCalled();

    mPostJSON.mockResolvedValueOnce({ reportId: REPORT_ID, files: [] });
    await expect(
      buildSubmitReport(IDENTITY)({ draft: draft() }),
    ).rejects.toThrow(/unexpected upload plan/i);
    expect(mPostJSON).toHaveBeenCalledTimes(1);
    expect(mSignedFetch).not.toHaveBeenCalled();
  });

  it("propagates a failed upload instead of reporting success", async () => {
    mPostJSON.mockResolvedValueOnce(presignResponse());
    mSignedFetch.mockResolvedValue(new Response(null, { status: 403 }));

    await expect(
      buildSubmitReport(IDENTITY)({ draft: draft() }),
    ).rejects.toThrow(/403/);
    expect(mPostJSON).toHaveBeenCalledTimes(1);
  });
});
