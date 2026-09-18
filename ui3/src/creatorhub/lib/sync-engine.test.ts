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

describe("sync engine", () => {
  test("markEditing shows editing until saveLocal enqueues, then the transport flushes to synced with a stable content hash", async () => {
    expect(hashBlob({ a: 1, b: 2 })).toBe(hashBlob({ b: 2, a: 1 }));
    expect(hashBlob({ a: 1 })).not.toBe(hashBlob({ a: 2 }));

    const engine = makeEngine();
    const seen: PushBody[] = [];
    engine.setTransport(async (_id, body) => {
      seen.push(body);
      return { ok: true, version: body.baseVersion + 1 };
    });
    engine.start();

    engine.markEditing("scene-a");
    expect(engine.getSummary().overall).toBe("editing");

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

  test("a conflict never loses local: resolve('local') re-pushes on the server version, resolve('cloud') adopts the server blob", async () => {
    const local = makeEngine();
    let calls = 0;
    local.setTransport(async (_id, body): Promise<PushResult> => {
      calls++;
      if (calls === 1) return { ok: false, conflict: true, version: 7, server: { from: "cloud" } };
      expect(body.baseVersion).toBe(7);
      return { ok: true, version: 8 };
    });
    local.start();
    await local.saveLocal("scene-b", { local: 1 });
    await waitFor(local, (x) => x.overall === "conflict");
    await local.resolveConflict("scene-b", "local");
    const kept = await waitFor(local, (x) => x.overall === "synced");
    expect(kept.scenes[0]?.version).toBe(8);
    expect(calls).toBe(2);

    const cloud = makeEngine();
    cloud.setTransport(async () => ({
      ok: false,
      conflict: true,
      version: 3,
      server: { winner: "cloud" },
    }));
    cloud.start();
    await cloud.saveLocal("scene-c", { winner: "local" });
    await waitFor(cloud, (x) => x.overall === "conflict");
    await cloud.resolveConflict("scene-c", "cloud");
    const adopted = await waitFor(cloud, (x) => x.overall === "synced" && x.pending === 0);
    expect(adopted.scenes[0]?.version).toBe(3);
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
});
