import { describe, expect, it } from "vitest";

import {
  aggregateManifests,
  createStepDebugger,
  diffSnapshots,
  formatNum,
  formatValue,
  leafChanges,
  orderEntityDiffs,
  parseDebugLogManifests,
  parseSceneStats,
  parseTickReply,
  type DebugManifest,
  type DebugSnapshot,
} from "./debugger";

const xf = (z: number) => ({
  position: { x: 8, y: 0, z },
  rotation: { x: 0, y: 0, z: 0, w: 1 },
  scale: { x: 1, y: 1, z: 1 },
});

describe("diffSnapshots", () => {
  it("reports changed components as leaf old\u{2192}new paths, components added on a surviving entity, and nothing for identical snapshots", () => {
    const prev: DebugSnapshot = { "512": { Transform: xf(13.4), TextShape: { text: "hi" } } };
    const next: DebugSnapshot = { "512": { Transform: xf(13.187), TextShape: { text: "hi" } } };
    const diffs = diffSnapshots(prev, next);
    expect(diffs).toHaveLength(1);
    const d = diffs[0]!;
    expect(d).toMatchObject({ id: "512", kind: "changed", unchanged: 1 });
    expect(d.comps).toHaveLength(1);
    expect(d.comps[0]!).toMatchObject({ name: "Transform", kind: "changed" });
    expect(d.comps[0]!.changes).toEqual([
      { path: "position.z", before: "13.4", after: "13.187" },
    ]);

    const grown: DebugSnapshot = { "512": { Transform: xf(1), VisibilityComponent: { visible: false } } };
    expect(diffSnapshots({ "512": { Transform: xf(1) } }, grown)[0]!.comps).toEqual([
      expect.objectContaining({ name: "VisibilityComponent", kind: "added" }),
    ]);

    const s: DebugSnapshot = { "512": { Transform: xf(2) }, "0": { EngineInfo: { tick: 5 } } };
    expect(diffSnapshots(s, JSON.parse(JSON.stringify(s)))).toEqual([]);
  });

  it("reports entities created and removed between ticks", () => {
    const prev: DebugSnapshot = { "512": { Transform: xf(1) } };
    const next: DebugSnapshot = {
      "512": { Transform: xf(1) },
      "600": { TextShape: { text: "TOWER DEFENSE" } },
    };
    const created = diffSnapshots(prev, next);
    expect(created).toHaveLength(1);
    expect(created[0]!).toMatchObject({ id: "600", kind: "new" });
    expect(created[0]!.comps[0]!).toMatchObject({ name: "TextShape", kind: "added" });
    expect(created[0]!.comps[0]!.value).toContain("TOWER DEFENSE");

    const removed = diffSnapshots(next, prev);
    expect(removed).toHaveLength(1);
    expect(removed[0]!).toMatchObject({ id: "600", kind: "gone" });
    expect(removed[0]!.comps[0]!).toMatchObject({ name: "TextShape", kind: "removed" });
  });

  it("orders authored/runtime entities before reserved engine entities", () => {
    const prev: DebugSnapshot = {
      "0": { EngineInfo: { tick: 1 } },
      "1": { Transform: xf(0) },
      "600": { Transform: xf(5) },
      "512": { Transform: xf(1) },
    };
    const next: DebugSnapshot = {
      "0": { EngineInfo: { tick: 2 } },
      "1": { Transform: xf(0.5) },
      "600": { Transform: xf(4.9) },
      "512": { Transform: xf(0.9) },
    };
    expect(diffSnapshots(prev, next).map((d) => d.id)).toEqual(["512", "600", "0", "1"]);
    expect(
      orderEntityDiffs([
        { id: "2", kind: "changed", comps: [], unchanged: 0 },
        { id: "700", kind: "changed", comps: [], unchanged: 0 },
      ]).map((d) => d.id),
    ).toEqual(["700", "2"]);
  });

  it("treats opaque custom-component strings as plain values and caps leaf changes on huge values", () => {
    const prev: DebugSnapshot = { "512": { "1030": "5:aGVsbG8=" } };
    const next: DebugSnapshot = { "512": { "1030": "6:aGVsbG8=" } };
    const diffs = diffSnapshots(prev, next);
    expect(diffs[0]!.comps[0]!).toMatchObject({ name: "1030", kind: "changed" });
    expect(diffs[0]!.comps[0]!.changes[0]!.path).toBe("");

    const big = (v: number) =>
      Object.fromEntries(Array.from({ length: 40 }, (_, i) => [`k${i}`, i + v]));
    expect(leafChanges(big(0), big(1)).length).toBeLessThan(40);
  });
});

describe("formatting", () => {
  it("formats numbers and nested values compactly and depth-capped", () => {
    expect(formatNum(13)).toBe("13");
    expect(formatNum(13.3866668)).toBe("13.387");
    expect(formatNum(-0.0004)).toBe("0");
    expect(formatValue({ position: { x: 1.5, y: 0, z: 13.4 } })).toContain("13.4");
    expect(formatValue([1, 2, 3, 4, 5, 6])).toContain("\u{2026}");
    expect(formatValue({ a: { b: { c: 1 } } })).not.toContain("c: 1");
  });
});

describe("console reply parsing", () => {
  it("parses tick_scene and scene_stats replies", () => {
    expect(parseTickReply("advancing 10 ticks from 123")).toBe(123);
    expect(parseTickReply("advancing 1 tick from 7")).toBe(7);
    expect(parseTickReply("scene is not frozen (use /freeze_scene first)")).toBeNull();

    const raw =
      "scene: 'My New Scene'\nhash: b64-abc\ntick: 456\nentities: 23\nstatus: blocked({\"frozen\"})\nbroken: false\nin_flight: false";
    expect(parseSceneStats(raw)).toEqual({
      tick: 456,
      frozen: true,
      title: "My New Scene",
      entities: 23,
    });
    expect(parseSceneStats("scene: 'x'\ntick: 3\nstatus: running").frozen).toBe(false);
  });
});

const manifestLine = (m: Partial<DebugManifest>) =>
  `[12.34] Log: one-dbg ${JSON.stringify({ game: "tower-defense", tick: 1, systems: [], created: [], handlers: 2, ...m })}`;

describe("parseDebugLogManifests", () => {
  it("extracts one-dbg lines from a scene_logs reply, skipping everything else, and returns [] without manifests", () => {
    const raw = [
      "[0.43] Log: [template-games] starting tower-defense",
      manifestLine({ tick: 7, systems: [{ name: "march creeps", ran: true, runs: 7 }] }),
      "[1.01] Log: [template-games] poll #3 status=200",
      manifestLine({ tick: 8, systems: [{ name: "march creeps", ran: true, runs: 8 }] }),
      "[1.10] Log: one-dbg {truncated garbage",
    ].join("\n");
    const ms = parseDebugLogManifests(raw);
    expect(ms).toHaveLength(2);
    expect(ms[0]!).toMatchObject({ game: "tower-defense", tick: 7 });
    expect(ms[1]!.systems[0]!).toEqual({ name: "march creeps", ran: true, runs: 8 });
    expect(parseDebugLogManifests("(no logs)")).toEqual([]);
  });
});

describe("aggregateManifests", () => {
  it("marks a system executed when it ran in any tick, keeps rows removed mid-step, falls back to the latest manifest, and is null without telemetry", () => {
    const batch: DebugManifest[] = [
      { game: "g", tick: 5, systems: [{ name: "a", ran: true, runs: 5 }, { name: "b", ran: false, runs: 0 }], created: [], handlers: 1 },
      { game: "g", tick: 6, systems: [{ name: "a", ran: false, runs: 5 }, { name: "b", ran: false, runs: 0 }], created: [], handlers: 1 },
    ];
    const view = aggregateManifests(batch, null);
    expect(view!.rows).toEqual([
      { name: "a", ran: true, runs: 5 },
      { name: "b", ran: false, runs: 0 },
    ]);
    expect(view!.harnessTick).toBe(6);

    const flash: DebugManifest[] = [
      { game: "g", tick: 5, systems: [{ name: "flash", ran: true, runs: 3 }], created: [], handlers: 0 },
      { game: "g", tick: 6, systems: [], created: [], handlers: 0 },
    ];
    expect(aggregateManifests(flash, null)!.rows).toEqual([{ name: "flash", ran: true, runs: 0 }]);

    const registry: DebugManifest = {
      game: "g", tick: 3, systems: [{ name: "a", ran: false, runs: 2 }], created: [], handlers: 0, registry: true,
    };
    const fallback = aggregateManifests([], registry);
    expect(fallback!.rows[0]!).toEqual({ name: "a", ran: false, runs: 2 });
    expect(fallback!.harnessTick).toBeNull();
    expect(aggregateManifests([], null)).toBeNull();
  });
});

interface FakeEngine {
  run: (cmd: string) => Promise<string>;
  calls: string[];
}

function fakeEngine(opts?: {
  startTick?: number;
  frozen?: boolean;
  snapshots?: DebugSnapshot[];
  refreezeLagPolls?: number;
  logs?: string[];
}): FakeEngine {
  let tick = opts?.startTick ?? 100;
  let frozen = opts?.frozen ?? true;
  let lag = 0;
  const snaps = opts?.snapshots ?? [{}];
  let snapIdx = 0;
  let logIdx = 0;
  const calls: string[] = [];
  const run = async (cmd: string): Promise<string> => {
    calls.push(cmd);
    if (cmd === "freeze_scene") {
      if (frozen) throw new Error("scene is already frozen");
      frozen = true;
      return `frozen at tick ${tick}`;
    }
    if (cmd.startsWith("tick_scene")) {
      if (!frozen) throw new Error("scene is not frozen (use /freeze_scene first)");
      const n = Number(cmd.split(" ")[1] || "1");
      const from = tick;
      tick += n;
      lag = opts?.refreezeLagPolls ?? 0;
      return `advancing ${n} tick${n === 1 ? "" : "s"} from ${from}`;
    }
    if (cmd === "scene_stats") {
      const state = lag > 0 ? (lag -= 1, "running") : 'blocked({"frozen"})';
      const shownTick = lag > 0 ? tick - 1 : tick;
      return `scene: 'T'\nhash: h\ntick: ${shownTick}\nentities: 2\nstatus: ${state}\nbroken: false\nin_flight: false`;
    }
    if (cmd === "crdt_snapshot") {
      const s = snaps[Math.min(snapIdx, snaps.length - 1)];
      snapIdx += 1;
      return JSON.stringify(s);
    }
    if (cmd.startsWith("scene_logs")) {
      const logs = opts?.logs ?? ["(no logs)"];
      const s = logs[Math.min(logIdx, logs.length - 1)]!;
      logIdx += 1;
      return s;
    }
    throw new Error(`unexpected cmd ${cmd}`);
  };
  return { run, calls };
}

const instant = () => Promise.resolve();

describe("createStepDebugger", () => {
  it("open() captures the baseline, then step() ticks, waits for the refreeze barrier and diffs against it", async () => {
    const eng = fakeEngine({
      startTick: 100,
      snapshots: [
        { "512": { Transform: xf(13.4) } },
        { "512": { Transform: xf(13.187) } },
      ],
      refreezeLagPolls: 3,
    });
    const dbg = createStepDebugger({ run: eng.run, sleep: instant });
    const opened = await dbg.open();
    expect(opened).toMatchObject({ tick: 100, frozen: true, entities: 1 });
    const res = await dbg.step(10);
    expect(res).toMatchObject({ fromTick: 100, tick: 110, count: 10, timedOut: false });
    expect(res!.entries).toHaveLength(1);
    expect(res!.entries[0]!.comps[0]!.changes[0]!).toEqual({
      path: "position.z",
      before: "13.4",
      after: "13.187",
    });
    expect(eng.calls.filter((c) => c === "scene_stats").length).toBeGreaterThan(2);
  });

  it("rejects overlapping steps but allows sequential ones, and stops after dispose()", async () => {
    const eng = fakeEngine({ snapshots: [{}, {}, {}] });
    const dbg = createStepDebugger({ run: eng.run, sleep: instant });
    await dbg.open();
    const results = await Promise.all([dbg.step(1), dbg.step(1)]);
    expect(results.filter((r) => r === null)).toHaveLength(1);
    expect(results.filter((r) => r !== null)).toHaveLength(1);
    expect(await dbg.step(1)).not.toBeNull();
    dbg.dispose();
    expect(await dbg.step(1)).toBeNull();
  });

  it("freezes an unfrozen scene on open, normalizes bad counts, and reports a barrier timeout honestly", async () => {
    const eng = fakeEngine({ frozen: false, startTick: 5, snapshots: [{}, {}] });
    const dbg = createStepDebugger({ run: eng.run, sleep: instant, maxPollMs: 200 });
    expect((await dbg.open()).frozen).toBe(true);
    expect((await dbg.step(1))!.fromTick).toBe(5);

    const slow = fakeEngine({ snapshots: [{}, {}], refreezeLagPolls: 100000 });
    const stuck = createStepDebugger({ run: slow.run, sleep: instant, maxPollMs: 120, pollIntervalMs: 60 });
    await stuck.open();
    const res = await stuck.step(0);
    expect(res).toMatchObject({ count: 1, timedOut: true });
  });

  it("reads harness manifests from scene_logs and dedupes across steps (ring re-serves lines)", async () => {
    const l1 = manifestLine({ tick: 5, systems: [{ name: "march creeps", ran: true, runs: 5 }] });
    const l2 = manifestLine({ tick: 6, systems: [{ name: "march creeps", ran: false, runs: 5 }] });
    const eng = fakeEngine({
      snapshots: [{}, {}, {}],
      logs: ["(no logs)", l1, `${l1}\n${l2}`],
    });
    const dbg = createStepDebugger({ run: eng.run, sleep: instant });
    expect((await dbg.open()).systems).toBeNull();
    const s1 = await dbg.step(1);
    expect(s1!.systems!.rows).toEqual([{ name: "march creeps", ran: true, runs: 5 }]);
    const s2 = await dbg.step(1);
    expect(s2!.systems!.rows).toEqual([{ name: "march creeps", ran: false, runs: 5 }]);
    expect(s2!.systems!.harnessTick).toBe(6);
  });
});
