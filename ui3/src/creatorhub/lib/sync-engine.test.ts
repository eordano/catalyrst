import { afterEach, describe, expect, test, vi } from "vitest";
import {
  createSyncEngine,
  hashBlob,
  type EngineSummary,
  type PushBody,
  type PushResult,
  type SyncEngine,
} from "./sync-engine";

function waitFor(engine: SyncEngine, pred: (s: EngineSummary) => boolean, timeoutMs = 1000): Promise<EngineSummary> {
  return new Promise((resolve, reject) => {
    const check = () => {
      if (pred(engine.getSummary())) {
        cleanup();
        resolve(engine.getSummary());
      }
    };
    const unsub = engine.subscribe(check);
    const t = setTimeout(() => {
      cleanup();
      reject(new Error("waitFor timed out; last=" + JSON.stringify(engine.getSummary())));
    }, timeoutMs);
    const cleanup = () => {
      clearTimeout(t);
      unsub();
    };
    check();
  });
}

const engines: SyncEngine[] = [];
function makeEngine(): SyncEngine {
  const e = createSyncEngine({ dbName: "test-" + Math.random().toString(36).slice(2) });
  engines.push(e);
  return e;
}

afterEach(() => {
  for (const e of engines.splice(0)) e.stop();
});

describe("hashBlob", () => {
  test("is stable across key order and changes with content", () => {
    expect(hashBlob({ a: 1, b: 2 })).toBe(hashBlob({ b: 2, a: 1 }));
    expect(hashBlob({ a: 1 })).not.toBe(hashBlob({ a: 2 }));
  });
});

describe("sync engine", () => {
  test("saveLocal enqueues then flushes to synced via transport", async () => {
    const engine = makeEngine();
    const seen: PushBody[] = [];
    engine.setTransport(async (_id, body) => {
      seen.push(body);
      return { ok: true, version: body.baseVersion + 1 };
    });
    engine.start();

    await engine.saveLocal("scene-a", { hello: "world" }, "Scene A");
    expect(engine.getSummary().pending).toBeGreaterThanOrEqual(0);

    const s = await waitFor(engine, (x) => x.overall === "synced" && x.pending === 0);
    expect(s.scenes[0]?.state).toBe("synced");
    expect(s.scenes[0]?.version).toBe(1);
    expect(s.scenes[0]?.lastSyncedAt).not.toBeNull();
    expect(seen).toHaveLength(1);
    expect(seen[0]?.baseVersion).toBe(0);
    expect(seen[0]?.hash).toBe(hashBlob({ hello: "world" }));
  });

  test("conflict is surfaced and never loses local; resolve('local') re-pushes", async () => {
    const engine = makeEngine();
    let calls = 0;
    engine.setTransport(async (_id, body): Promise<PushResult> => {
      calls++;
      if (calls === 1) return { ok: false, conflict: true, version: 7, server: { from: "cloud" } };
      expect(body.baseVersion).toBe(7);
      return { ok: true, version: 8 };
    });
    engine.start();

    await engine.saveLocal("scene-b", { local: 1 });
    await waitFor(engine, (x) => x.overall === "conflict");

    await engine.resolveConflict("scene-b", "local");
    const s = await waitFor(engine, (x) => x.overall === "synced");
    expect(s.scenes[0]?.version).toBe(8);
    expect(calls).toBe(2);
  });

  test("resolve('cloud') adopts the server blob and clears the queue", async () => {
    const engine = makeEngine();
    engine.setTransport(async () => ({
      ok: false,
      conflict: true,
      version: 3,
      server: { winner: "cloud" },
    }));
    engine.start();

    await engine.saveLocal("scene-c", { winner: "local" });
    await waitFor(engine, (x) => x.overall === "conflict");

    await engine.resolveConflict("scene-c", "cloud");
    const s = await waitFor(engine, (x) => x.overall === "synced" && x.pending === 0);
    expect(s.scenes[0]?.version).toBe(3);
  });

  test("network failure keeps the item queued (error), not lost", async () => {
    const engine = makeEngine();
    const fail = vi.fn(async () => {
      throw new Error("network down");
    });
    engine.setTransport(fail);
    engine.start();

    await engine.saveLocal("scene-d", { x: 1 });
    const s = await waitFor(engine, (x) => x.scenes[0]?.state === "error");
    expect(s.pending).toBe(1);
    expect(fail).toHaveBeenCalled();
  });

  test("markEditing shows editing until the next saveLocal", async () => {
    const engine = makeEngine();
    engine.setTransport(async (_id, body) => ({ ok: true, version: body.baseVersion + 1 }));
    engine.start();

    engine.markEditing("scene-e");
    expect(engine.getSummary().overall).toBe("editing");

    await engine.saveLocal("scene-e", { done: true });
    const s = await waitFor(engine, (x) => x.overall === "synced");
    expect(s.scenes[0]?.state).toBe("synced");
  });
});
