import { describe, expect, it } from "vitest";

/*
 * Every foundry action refuses bad input before it touches the database:
 * an unreadable body is a 400 with the same plain sentence everywhere, and a
 * cookieless write is a 429 that names the session cookie. These paths are
 * reachable without a database by construction — the formData guard and
 * requireCookie run before any query — and this file keeps them that way.
 */

import { action as exchangeAction } from "./foundry.exchange";
import { action as peopleAction } from "./foundry.people";
import { action as personaAction } from "./foundry.persona";
import { action as playAction } from "./foundry.play_.$slug";
import { action as sessionsAction } from "./foundry.sessions";
import { action as stewardshipAction } from "./foundry.stewardship";

type ActionResult = {
  init?: { status?: number } | null;
  data: { ok: boolean; error: string | null };
};

type AnyAction = (args: {
  request: Request;
  params: Record<string, string>;
  context: never;
}) => Promise<unknown>;

const ACTIONS: readonly {
  name: string;
  action: AnyAction;
  path: string;
  params: Record<string, string>;
}[] = [
  {
    name: "exchange",
    action: exchangeAction as unknown as AnyAction,
    path: "/foundry/exchange",
    params: {},
  },
  {
    name: "play",
    action: playAction as unknown as AnyAction,
    path: "/foundry/play/nope",
    params: { slug: "nope" },
  },
  {
    name: "sessions",
    action: sessionsAction as unknown as AnyAction,
    path: "/foundry/sessions",
    params: {},
  },
  {
    name: "persona",
    action: personaAction as unknown as AnyAction,
    path: "/foundry/persona",
    params: {},
  },
  {
    name: "stewardship",
    action: stewardshipAction as unknown as AnyAction,
    path: "/foundry/stewardship",
    params: {},
  },
  {
    name: "people",
    action: peopleAction as unknown as AnyAction,
    path: "/foundry/people",
    params: {},
  },
];

function post(path: string, contentType: string, body: string): Request {
  return new Request(`https://sites.test${path}`, {
    method: "POST",
    headers: { "content-type": contentType },
    body,
  });
}

async function run(
  entry: (typeof ACTIONS)[number],
  request: Request,
): Promise<ActionResult> {
  return (await entry.action({
    request,
    params: entry.params,
    context: {} as never,
  })) as ActionResult;
}

describe.each(ACTIONS)("POST $path", (entry) => {
  it("answers a JSON body with a 400, not a 500", async () => {
    const res = await run(
      entry,
      post(entry.path, "application/json", '{"x":1}'),
    );
    expect(res.init?.status).toBe(400);
    expect(res.data.error).toBe("Could not read the form.");
  });

  it("answers a truncated multipart body with the same 400", async () => {
    const res = await run(
      entry,
      post(
        entry.path,
        "multipart/form-data; boundary=b",
        "--b\r\nContent-Disposition: form-data; na",
      ),
    );
    expect(res.init?.status).toBe(400);
    expect(res.data.error).toBe("Could not read the form.");
  });

  it("refuses a cookieless write with a 429 that names the session cookie", async () => {
    const res = await run(
      entry,
      post(entry.path, "application/x-www-form-urlencoded", "intent=pledge"),
    );
    expect(res.init?.status).toBe(429);
    expect(res.data.ok).toBe(false);
    expect(res.data.error).toContain("session cookie");
  });
});
