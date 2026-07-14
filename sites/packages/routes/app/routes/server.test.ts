import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { clearProbeSnapshot } from "@data/lib/operator/probe.server";
import { SERVICES } from "@data/lib/operator/registry";

import { action, loader } from "./server";

let dir: string;
let file: string;

beforeEach(async () => {
  dir = await mkdtemp(join(tmpdir(), "server-route-"));
  file = join(dir, "operator.env");
  vi.stubEnv("CATALYRST_OPERATOR_ENV_FILE", file);
  vi.stubEnv("ADMIN_WALLETS", "");
  vi.stubEnv("CATALYRST_ENABLED_SERVICES", "");
  clearProbeSnapshot();
});

afterEach(async () => {
  vi.unstubAllEnvs();
  await rm(dir, { recursive: true, force: true });
});

function get(search = ""): Request {
  return new Request(`https://node.example/server${search}`);
}

function post(fields: Record<string, string>, headers: Record<string, string> = {}): Request {
  const body = new URLSearchParams(fields);
  return new Request("https://node.example/server", {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded", ...headers },
    body,
  });
}

function callLoader(request: Request) {
  return loader({ request } as unknown as Parameters<typeof loader>[0]);
}

function callAction(request: Request) {
  return action({ request } as unknown as Parameters<typeof action>[0]);
}

describe("/server loader", () => {
  it("answers with every registered service even when nothing is configured", async () => {
    const data = await callLoader(get());
    expect(data.authorized).toBe(true);
    if (!data.authorized) return;
    expect(data.mode).toBe("edge");
    expect(data.services.length).toBe(SERVICES.length);
    for (const s of data.services) {
      expect(["ok", "answering", "down"]).toContain(s.state);
      if (s.state === "down" || s.state === "answering") {
        expect(s.actionables.length).toBeGreaterThan(0);
      }
    }
    expect(data.env.ok).toBe(true);
  });

  it("renders unenabled services as off, unprobed and without commands", async () => {
    vi.stubEnv("CATALYRST_ENABLED_SERVICES", "content,livekit-signaling");
    const data = await callLoader(get());
    if (!data.authorized) throw new Error("expected authorized");
    const off = data.services.filter((s) => s.state === "off");
    expect(off.length).toBe(SERVICES.length - 2);
    for (const s of off) {
      expect(s.actionables).toEqual([]);
      expect(s.detail).toContain("not enabled");
    }
    expect(data.services.filter((s) => s.state !== "off").map((s) => s.key).sort()).toEqual([
      "content",
      "livekit-signaling",
    ]);
  });

  it("enables a bundle carrier when any of its member services is enabled", async () => {
    vi.stubEnv("CATALYRST_ENABLED_SERVICES", "content,places,badges");
    const data = await callLoader(get());
    if (!data.authorized) throw new Error("expected authorized");
    const on = data.services.filter((s) => s.state !== "off").map((s) => s.key).sort();
    expect(on).toEqual(["content", "explore", "social"]);
  });

  it("scopes a recheck to the named services and serves the rest from the snapshot", async () => {
    const first = await callLoader(get());
    if (!first.authorized) throw new Error("expected authorized");
    await new Promise((r) => setTimeout(r, 20));
    const scoped = await callLoader(get("?recheck=livekit,nats"));
    if (!scoped.authorized) throw new Error("expected authorized");
    expect(scoped.services.length).toBe(SERVICES.length);
    const untouched = scoped.services.filter(
      (s) => s.key !== "livekit" && s.key !== "nats",
    );
    expect(untouched.some((s) => s.ageMs > 0)).toBe(true);
  });

  it("requires a wallet once ADMIN_WALLETS is set", async () => {
    vi.stubEnv("ADMIN_WALLETS", "0xabc");
    const data = await callLoader(get());
    expect(data.authorized).toBe(false);
  });

  it("never sends a secret's persisted value to the client", async () => {
    await writeFile(file, "SOME_API_TOKEN=hunter2\nPLAIN_SETTING=visible\n");
    const data = await callLoader(get());
    if (!data.authorized || !data.env.ok) throw new Error("expected authorized env panel");
    const secret = data.env.data.rows.find((r) => r.name === "SOME_API_TOKEN");
    const plain = data.env.data.rows.find((r) => r.name === "PLAIN_SETTING");
    expect(secret?.secret).toBe(true);
    expect(secret?.fileValue).toBe("");
    expect(JSON.stringify(data)).not.toContain("hunter2");
    expect(plain?.fileValue).toBe("visible");
  });
});

describe("/server action", () => {
  it("persists env-save and env-delete round trips", async () => {
    const saved = await callAction(post({ intent: "env-save", name: "MY_SETTING", value: "on" }));
    expect(saved.ok).toBe(true);
    expect(await readFile(file, "utf8")).toBe("MY_SETTING=on\n");
    const removed = await callAction(post({ intent: "env-delete", name: "MY_SETTING" }));
    expect(removed.ok).toBe(true);
    expect(await readFile(file, "utf8")).toBe("");
  });

  it("refuses an empty replacement for a secret", async () => {
    const r = await callAction(post({ intent: "env-save", name: "SOME_API_TOKEN", value: "" }));
    expect(r.ok).toBe(false);
    expect(r.message).toContain("secret");
  });

  it("refuses cross-site writes", async () => {
    const r = await callAction(
      post({ intent: "env-save", name: "X_Y", value: "1" }, { "sec-fetch-site": "cross-site" }),
    );
    expect(r.ok).toBe(false);
  });

  it("refuses writes when wallet auth is on and absent", async () => {
    vi.stubEnv("ADMIN_WALLETS", "0xabc");
    const r = await callAction(post({ intent: "env-save", name: "X_Y", value: "1" }));
    expect(r.ok).toBe(false);
  });

  it("names the rejected intent", async () => {
    const r = await callAction(post({ intent: "reboot" }));
    expect(r.ok).toBe(false);
    expect(r.message).toContain("reboot");
  });
});
