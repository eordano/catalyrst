import { describe, expect, it, vi } from "vitest";
import { cloneValue, createHistory, type HistoryEntry } from "./history";

type WriteLog = { entity: string; name: string; value: unknown }[];

function makeEngine(maxSteps?: number) {
  const log: WriteLog = [];
  const onChange = vi.fn();
  const h = createHistory(
    (entity, name, value) => log.push({ entity, name, value }),
    onChange,
    maxSteps,
  );
  return { h, log, onChange };
}

const entry = (over: Partial<HistoryEntry> = {}): HistoryEntry => ({
  entity: "512",
  name: "core::Material",
  before: { roughness: 0.5 },
  after: { roughness: 1 },
  ...over,
});

describe("createHistory", async () => {
  it("undo replays before and redo replays after through the write path; an empty stack is a safe no-op", async () => {
    const { h, log } = makeEngine();
    expect(await h.undo()).toBe(false);
    expect(await h.redo()).toBe(false);
    expect(log).toEqual([]);

    h.push([entry()]);
    expect(h.canUndo()).toBe(true);
    expect(h.canRedo()).toBe(false);

    expect(await h.undo()).toBe(true);
    expect(log).toEqual([{ entity: "512", name: "core::Material", value: { roughness: 0.5 } }]);
    expect(h.canUndo()).toBe(false);
    expect(h.canRedo()).toBe(true);

    expect(await h.redo()).toBe(true);
    expect(log[1]).toEqual({ entity: "512", name: "core::Material", value: { roughness: 1 } });
    expect(h.canUndo()).toBe(true);
    expect(h.canRedo()).toBe(false);
  });

  it("a fresh push clears the redo branch, empty batches are ignored, and every real change notifies", async () => {
    const { h, onChange } = makeEngine();
    h.push([]);
    h.push(null as unknown as HistoryEntry[]);
    expect(h.canUndo()).toBe(false);
    expect(onChange).not.toHaveBeenCalled();

    const notifies = async (change: () => unknown) => {
      onChange.mockClear();
      await change();
      expect(onChange).toHaveBeenCalled();
    };
    await notifies(() => h.push([entry()]));
    await notifies(() => h.undo());
    expect(h.canRedo()).toBe(true);
    await notifies(() => h.push([entry({ after: { roughness: 0.25 } })]));
    expect(h.canRedo()).toBe(false);
    await notifies(() => h.undo());
    await notifies(() => h.redo());
    await notifies(() => h.clear());
    expect(h.canUndo()).toBe(false);
    expect(h.canRedo()).toBe(false);
  });

  it("undefined values mean create/delete: undo of a first write deletes, undo of a removal restores", async () => {
    const { h, log } = makeEngine();
    h.push([entry({ before: undefined, after: { visible: true }, name: "VisibilityComponent" })]);
    await h.undo();
    expect(log[0]).toEqual({ entity: "512", name: "VisibilityComponent", value: undefined });
    h.push([entry({ before: { src: "a.glb" }, after: undefined, name: "GltfContainer" })]);
    await h.undo();
    expect(log[1]).toEqual({ entity: "512", name: "GltfContainer", value: { src: "a.glb" } });
    await h.redo();
    expect(log[2]).toEqual({ entity: "512", name: "GltfContainer", value: undefined });
  });

  it("batches replay every entry and the undo depth is capped at maxSteps, oldest dropped first", async () => {
    const multi = makeEngine();
    multi.h.push([
      entry({ entity: "1", name: "Transform", before: { x: 0 }, after: { x: 5 } }),
      entry({ entity: "2", name: "Transform", before: { x: 1 }, after: { x: 6 } }),
    ]);
    await multi.h.undo();
    expect(multi.log).toEqual([
      { entity: "1", name: "Transform", value: { x: 0 } },
      { entity: "2", name: "Transform", value: { x: 1 } },
    ]);

    const { h, log } = makeEngine(3);
    for (let i = 0; i < 5; i += 1) {
      h.push([entry({ before: { i }, after: { i: i + 100 } })]);
    }
    let undone = 0;
    while (await h.undo()) undone += 1;
    expect(undone).toBe(3);
    expect(log.map((w) => (w.value as { i: number }).i)).toEqual([4, 3, 2]);
  });

  it("pushes during a replay are suppressed (no self-recording loops)", async () => {
    const log: WriteLog = [];
    const h = createHistory((entity, name, value) => {
      log.push({ entity, name, value });
      h.push([entry({ before: { echoed: true } })]);
      expect(h.isSuppressed()).toBe(true);
    });
    h.push([entry()]);
    await h.undo();
    expect(h.canUndo()).toBe(false);
    expect(h.canRedo()).toBe(true);
    expect(log).toHaveLength(1);
  });
});

describe("cloneValue", async () => {
  it("deep-clones objects without aliasing and passes primitives through", async () => {
    const src = { position: { x: 1 } };
    const copy = cloneValue(src);
    expect(copy).toEqual(src);
    expect(copy).not.toBe(src);
    expect(copy.position).not.toBe(src.position);
    expect(cloneValue(undefined)).toBeUndefined();
    expect(cloneValue(null)).toBeNull();
    expect(cloneValue(7)).toBe(7);
  });
});

it("keeps stacks unchanged until acknowledgment and refuses overlapping replays", async () => {
  let acknowledge!: () => void;
  const history = createHistory(() => new Promise<void>(resolve => { acknowledge = resolve; }));
  history.push([entry()]);
  const pending = history.undo();
  expect(history.canUndo()).toBe(false);
  expect(history.canRedo()).toBe(false);
  expect(await history.redo()).toBe(false);
  acknowledge();
  expect(await pending).toBe(true);
  expect(history.canRedo()).toBe(true);
});

it("rolls back a partially applied undo and leaves its history available for retry", async () => {
  const state = new Map([["512", 8], ["513", 9]]);
  let rejectSecond = true;
  const history = createHistory(async (entity, _name, value) => {
    if (entity === "513" && rejectSecond) throw new Error("Scene disconnected");
    state.set(entity, value as number);
  });
  history.push([entry({ before: 1, after: 8 }), entry({ entity: "513", before: 2, after: 9 })]);
  await expect(history.undo()).rejects.toThrow("Scene disconnected");
  expect([...state.values()]).toEqual([8, 9]);
  expect(history.canUndo()).toBe(true);
  expect(history.canRedo()).toBe(false);
  rejectSecond = false;
  await history.undo();
  expect([...state.values()]).toEqual([1, 2]);
  await history.redo();
  expect([...state.values()]).toEqual([8, 9]);
});
