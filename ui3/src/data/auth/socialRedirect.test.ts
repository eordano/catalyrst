import { afterEach, describe, expect, test, vi } from "vitest";

import { getEngineAuthState, signOutEngineAuth } from "./engineLogin";
import { AUTH_RESULT_PARAM, completeSocialRedirectLogin } from "./socialRedirect";

const WALLET = "0x00000000000000000000000000000000000000aa";

function setUrl(search: string) {
  window.history.replaceState(null, "", `/${search}`);
}

afterEach(() => {
  vi.unstubAllGlobals();
  signOutEngineAuth();
  setUrl("");
});

describe("completeSocialRedirectLogin", () => {
  test("no authResult param: no-op", async () => {
    setUrl("");
    await expect(completeSocialRedirectLogin()).resolves.toBe(false);
  });

  test("valid authResult: signs an identity via the proxy and stashes it", async () => {
    vi.stubGlobal(
      "fetch",
      async () =>
        new Response(
          JSON.stringify({ signature: `0x${"11".repeat(64)}1b` }),
          { status: 200 },
        ),
    );
    const authResult = JSON.stringify({ token: "jwt", walletAddress: WALLET });
    setUrl(`?${AUTH_RESULT_PARAM}=${encodeURIComponent(authResult)}&realm=x`);

    await expect(completeSocialRedirectLogin()).resolves.toBe(true);

    const params = new URLSearchParams(window.location.search);
    expect(params.get(AUTH_RESULT_PARAM)).toBeNull();
    expect(params.get("realm")).toBe("x");

    expect(getEngineAuthState()).toEqual({ status: "pending", address: WALLET });
  });

  test("garbage authResult: param stripped, no sign-in, no crash", async () => {
    setUrl(`?${AUTH_RESULT_PARAM}=%7Bnope`);
    await expect(completeSocialRedirectLogin()).resolves.toBe(false);
    expect(
      new URLSearchParams(window.location.search).get(AUTH_RESULT_PARAM),
    ).toBeNull();
    expect(getEngineAuthState().status).toBe("none");
  });
});
