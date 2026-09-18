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

async function message(res: Response): Promise<string> {
  return ((await res.json()) as { message: string }).message;
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
  it("fails closed with 503 without a submit endpoint or when it points at the live Decentraland DAO API", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");

    const unconfigured = await action(post("catalyst"));
    expect(unconfigured.status).toBe(503);
    expect(await message(unconfigured)).toMatch(/GOVERNANCE_SUBMIT_URL/);

    process.env.GOVERNANCE_SUBMIT_URL = "https://governance.decentraland.org/api";
    const live = await action(post("catalyst"));
    expect(live.status).toBe(503);
    expect(await message(live)).toMatch(/refusing to forward/);

    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("rejects an unknown kind, an unsigned request and a non-POST method before touching the network", async () => {
    process.env.GOVERNANCE_SUBMIT_URL = "http://127.0.0.1:5151";
    const fetchSpy = vi.spyOn(globalThis, "fetch");

    expect((await action(post("grant"))).status).toBe(404);
    expect((await action(post("catalyst", { "content-type": "application/json" }))).status).toBe(401);
    const get = await action({
      request: new Request("https://sites.test/api/governance/proposals/catalyst", {
        method: "GET",
      }),
      params: { kind: "catalyst" },
    });
    expect(get.status).toBe(405);

    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("forwards the auth chain to the configured backend and relays its answer, reporting an unreachable backend as 502", async () => {
    process.env.GOVERNANCE_SUBMIT_URL = "http://127.0.0.1:5151/";
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(JSON.stringify({ id: "prop-1" }), {
        status: 201,
        headers: { "content-type": "application/json" },
      }),
    );

    const relayed = await action(post("council-decision-veto"));
    expect(relayed.status).toBe(201);
    expect(await relayed.json()).toEqual({ id: "prop-1" });
    const [url, init] = fetchSpy.mock.calls[0];
    expect(String(url)).toBe("http://127.0.0.1:5151/proposals/council-decision-veto");
    const sent = (init as RequestInit).headers as Headers;
    expect(sent.get("x-identity-auth-chain-0")).toBe(SIGNED_HEADERS["x-identity-auth-chain-0"]);
    expect(sent.get("x-identity-timestamp")).toBe(SIGNED_HEADERS["x-identity-timestamp"]);

    fetchSpy.mockRejectedValueOnce(new Error("ECONNREFUSED"));
    const unreachable = await action(post("tender"));
    expect(unreachable.status).toBe(502);
    expect(await message(unreachable)).toMatch(/ECONNREFUSED/);
    expect(fetchSpy).toHaveBeenCalledTimes(2);
  });
});
