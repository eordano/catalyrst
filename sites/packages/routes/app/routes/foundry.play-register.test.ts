import { describe, expect, it } from "vitest";

/*
 * The registration route's own refusals, all reachable without a database by
 * construction: the formData guard, requireCookie and registerScene's field
 * validation run before any query. The exact sentences double as ordering
 * proof — with no FOUNDRY_DATABASE_URL set, a check that had slipped behind
 * the data layer would answer "The program database is not configured."
 */

import { action } from "./foundry.play_.register";

type ActionResult = {
  init?: { status?: number } | null;
  data: { ok: boolean; slug: string | null; error: string | null };
};

const FIELDS = {
  intent: "register",
  id: "lantern-relay",
  title: "Lantern Relay",
  repoPath: "",
  gddDocId: "",
  sourceNote: "route-test provenance",
};

function post(fields: Record<string, string>, cookie = true): Request {
  return new Request("https://sites.test/foundry/play/register", {
    method: "POST",
    headers: {
      "content-type": "application/x-www-form-urlencoded",
      ...(cookie ? { cookie: "sid=register-test-sid" } : {}),
    },
    body: new URLSearchParams(fields).toString(),
  });
}

async function run(request: Request): Promise<ActionResult> {
  const result = await (action as (a: unknown) => Promise<unknown>)({
    request,
    params: {},
    context: {},
  });
  // Plain returns and react-router data() results, in one shape.
  if (result && typeof result === "object" && "data" in result) {
    return result as ActionResult;
  }
  return { data: result } as ActionResult;
}

describe("POST /foundry/play/register", () => {
  it("answers a JSON body with a 400, not a 500", async () => {
    const res = await run(
      new Request("https://sites.test/foundry/play/register", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: '{"x":1}',
      }),
    );
    expect(res.init?.status).toBe(400);
    expect(res.data.error).toBe("Could not read the form.");
  });

  it("refuses a cookieless write with the 429 that names the session cookie", async () => {
    const res = await run(post(FIELDS, false));
    expect(res.init?.status).toBe(429);
    expect(res.data.error).toContain("session cookie");
  });

  it("answers an unknown intent honestly", async () => {
    const res = await run(post({ ...FIELDS, intent: "make-it-so" }));
    expect(res.data.ok).toBe(false);
    expect(res.data.error).toBe("Unknown action.");
  });

  it("refuses malformed fields before the data layer, naming the fix", async () => {
    const badId = await run(post({ ...FIELDS, id: "Bad Slug!" }));
    expect(badId.init?.status).toBe(409);
    expect(badId.data.error).toContain("lowercase letters, digits and dashes");

    const noTitle = await run(post({ ...FIELDS, title: " " }));
    expect(noTitle.init?.status).toBe(409);
    expect(noTitle.data.error).toBe("Give the game a title.");

    const noNote = await run(post({ ...FIELDS, sourceNote: "" }));
    expect(noNote.init?.status).toBe(409);
    expect(noNote.data.error).toContain("where this game comes from");
  });

  it("a well-formed registration reaches the data layer", async () => {
    const res = await run(post(FIELDS));
    // No database in this suite: reaching registerScene's transaction is the
    // observable outcome.
    expect(res.data.ok).toBe(false);
    expect(res.data.error).toBe("The program database is not configured.");
  });
});
