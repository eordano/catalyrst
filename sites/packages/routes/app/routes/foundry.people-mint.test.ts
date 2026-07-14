import { describe, expect, it } from "vitest";

/*
 * The mint form's own refusals, all reachable without a database by
 * construction: requireCookie, the mintable-role fence and the expiry checks
 * run before mintInvite ever touches a pool. Each assertion on the exact
 * sentence is also proof of the ordering — with no FOUNDRY_DATABASE_URL set, a
 * check that had slipped behind the data layer would answer "The program
 * database is not configured." instead.
 */

import { action } from "./foundry.people";

type ActionResult = {
  init?: { status?: number } | null;
  data: { ok: boolean; error: string | null };
};

function post(fields: Record<string, string>, cookie = true): Request {
  return new Request("https://sites.test/foundry/people", {
    method: "POST",
    headers: {
      "content-type": "application/x-www-form-urlencoded",
      ...(cookie ? { cookie: "sid=mint-test-sid" } : {}),
    },
    body: new URLSearchParams(fields).toString(),
  });
}

async function run(request: Request): Promise<ActionResult> {
  return (await (action as (a: unknown) => Promise<unknown>)({
    request,
    params: {},
    context: {},
  })) as ActionResult;
}

describe("POST /foundry/people intent=mint_invite", () => {
  it("refuses a cookieless mint with the 429 that names the session cookie", async () => {
    const res = await run(post({ intent: "mint_invite", role: "start" }, false));
    expect(res.init?.status).toBe(429);
    expect(res.data.error).toContain("session cookie");
  });

  it("refuses to mint an operator invite before the data layer", async () => {
    for (const role of ["admin", "operator", "", "root"]) {
      const res = await run(post({ intent: "mint_invite", role }));
      expect(res.init?.status).toBe(409);
      expect(res.data.error).toBe(
        "This form mints host, create and start invites only.",
      );
    }
  });

  it("refuses an unreadable expiry and one already past", async () => {
    const garbled = await run(
      post({ intent: "mint_invite", role: "start", expires: "not-a-day" }),
    );
    expect(garbled.init?.status).toBe(409);
    expect(garbled.data.error).toBe("The expiry is not a readable date.");

    const past = await run(
      post({ intent: "mint_invite", role: "start", expires: "2001-01-01" }),
    );
    expect(past.init?.status).toBe(409);
    expect(past.data.error).toContain("already past");
  });

  it("a well-formed mint reaches the data layer (and nothing shortcuts it)", async () => {
    const res = await run(
      post({ intent: "mint_invite", role: "create", note: "route test" }),
    );
    // No database in this suite: reaching mintInvite is the observable outcome.
    expect(res.data.ok).toBe(false);
    expect(res.data.error).toBe("The program database is not configured.");
  });
});
