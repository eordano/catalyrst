import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { action } from "./api.governance.proposals.$kind";

const SIGNED_HEADERS = {
  "x-identity-timestamp": "1753500000000",
  "x-identity-auth-chain-0": JSON.stringify({ type: "SIGNER", payload: "0xabc", signature: "" }),
  "x-identity-metadata": "{}",
  "content-type": "application/json",
};

function post(kind: string, headers: Record<string, string> = SIGNED_HEADERS): {
  request: Request;
  params: Record<string, string>;
} {
  return {
    request: new Request(`https://sites.test/api/governance/proposals/${kind}`, {
      method: "POST",
      headers,
      body: JSON.stringify({ type: "catalyst_add" }),
    }),
    params: { kind },
  };
}

const originalSubmitUrl = process.env.GOVERNANCE_SUBMIT_URL;

beforeEach(() => {
  vi.restoreAllMocks();
  delete process.env.GOVERNANCE_SUBMIT_URL;
});

afterEach(() => {
  if (originalSubmitUrl === undefined) delete process.env.GOVERNANCE_SUBMIT_URL;
  else process.env.GOVERNANCE_SUBMIT_URL = originalSubmitUrl;
});

describe("POST /api/governance/proposals/:kind", () => {
  it("fails closed with 503 when no submit endpoint is configured", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");
    const res = await action(post("catalyst"));

    expect(res.status).toBe(503);
    expect(((await res.json()) as { message: string }).message).toMatch(
      /GOVERNANCE_SUBMIT_URL/,
    );
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("refuses to forward writes to the live Decentraland DAO API", async () => {
    process.env.GOVERNANCE_SUBMIT_URL = "https://governance.decentraland.org/api";
    const fetchSpy = vi.spyOn(globalThis, "fetch");

    const res = await action(post("catalyst"));

    expect(res.status).toBe(503);
    expect(((await res.json()) as { message: string }).message).toMatch(
      /refusing to forward/,
    );
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("rejects an unknown proposal kind before touching the network", async () => {
    process.env.GOVERNANCE_SUBMIT_URL = "http://127.0.0.1:5151";
    const fetchSpy = vi.spyOn(globalThis, "fetch");

    const res = await action(post("grant"));

    expect(res.status).toBe(404);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("rejects an unsigned request", async () => {
    process.env.GOVERNANCE_SUBMIT_URL = "http://127.0.0.1:5151";
    const fetchSpy = vi.spyOn(globalThis, "fetch");

    const res = await action(post("catalyst", { "content-type": "application/json" }));

    expect(res.status).toBe(401);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("forwards the auth chain to the configured backend and relays its answer", async () => {
    process.env.GOVERNANCE_SUBMIT_URL = "http://127.0.0.1:5151/";
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(
        new Response(JSON.stringify({ id: "prop-1" }), {
          status: 201,
          headers: { "content-type": "application/json" },
        }),
      );

    const res = await action(post("council-decision-veto"));

    expect(res.status).toBe(201);
    expect(await res.json()).toEqual({ id: "prop-1" });
    const [url, init] = fetchSpy.mock.calls[0];
    expect(String(url)).toBe("http://127.0.0.1:5151/proposals/council-decision-veto");
    const sent = (init as RequestInit).headers as Headers;
    expect(sent.get("x-identity-auth-chain-0")).toBe(
      SIGNED_HEADERS["x-identity-auth-chain-0"],
    );
    expect(sent.get("x-identity-timestamp")).toBe(SIGNED_HEADERS["x-identity-timestamp"]);
  });

  it("reports an unreachable backend as 502 instead of a success", async () => {
    process.env.GOVERNANCE_SUBMIT_URL = "http://127.0.0.1:5151";
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("ECONNREFUSED"));

    const res = await action(post("tender"));

    expect(res.status).toBe(502);
    expect(((await res.json()) as { message: string }).message).toMatch(/ECONNREFUSED/);
  });

  it("rejects non-POST methods", async () => {
    const res = await action({
      request: new Request("https://sites.test/api/governance/proposals/catalyst", {
        method: "GET",
      }),
      params: { kind: "catalyst" },
    });
    expect(res.status).toBe(405);
  });
});
