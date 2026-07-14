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

describe("createHistory", () => {
  it("undo replays before, redo replays after, through the write path", () => {
    const { h, log } = makeEngine();
    h.push([entry()]);
    expect(h.canUndo()).toBe(true);
    expect(h.canRedo()).toBe(false);

    expect(h.undo()).toBe(true);
    expect(log).toEqual([{ entity: "512", name: "core::Material", value: { roughness: 0.5 } }]);
    expect(h.canUndo()).toBe(false);
    expect(h.canRedo()).toBe(true);

    expect(h.redo()).toBe(true);
    expect(log[1]).toEqual({ entity: "512", name: "core::Material", value: { roughness: 1 } });
    expect(h.canUndo()).toBe(true);
    expect(h.canRedo()).toBe(false);
  });

  it("undo/redo on an empty stack is a safe no-op", () => {
    const { h, log } = makeEngine();
    expect(h.undo()).toBe(false);
    expect(h.redo()).toBe(false);
    expect(log).toEqual([]);
  });

  it("a fresh push clears the redo branch", () => {
    const { h } = makeEngine();
    h.push([entry()]);
    h.undo();
    expect(h.canRedo()).toBe(true);
    h.push([entry({ after: { roughness: 0.25 } })]);
    expect(h.canRedo()).toBe(false);
  });

  it("undefined values mean create/delete: undo of a first-write deletes, undo of a removal restores", () => {
    const { h, log } = makeEngine();
    h.push([entry({ before: undefined, after: { visible: true }, name: "VisibilityComponent" })]);
    h.undo();
    expect(log[0]).toEqual({ entity: "512", name: "VisibilityComponent", value: undefined });
    h.push([entry({ before: { src: "a.glb" }, after: undefined, name: "GltfContainer" })]);
    h.undo();
    expect(log[1]).toEqual({ entity: "512", name: "GltfContainer", value: { src: "a.glb" } });
    h.redo();
    expect(log[2]).toEqual({ entity: "512", name: "GltfContainer", value: undefined });
  });

  it("batches (multi-entity gizmo drags) replay every entry", () => {
    const { h, log } = makeEngine();
    h.push([
      entry({ entity: "1", name: "Transform", before: { x: 0 }, after: { x: 5 } }),
      entry({ entity: "2", name: "Transform", before: { x: 1 }, after: { x: 6 } }),
    ]);
    h.undo();
    expect(log).toEqual([
      { entity: "1", name: "Transform", value: { x: 0 } },
      { entity: "2", name: "Transform", value: { x: 1 } },
    ]);
  });

  it("pushes during a replay are suppressed (no self-recording loops)", () => {
    const log: WriteLog = [];
    const h = createHistory((entity, name, value) => {
      log.push({ entity, name, value });
      h.push([entry({ before: { echoed: true } })]);
      expect(h.isSuppressed()).toBe(true);
    });
    h.push([entry()]);
    h.undo();
    expect(h.canUndo()).toBe(false);
    expect(h.canRedo()).toBe(true);
    expect(log).toHaveLength(1);
  });

  it("empty and non-array batches are ignored", () => {
    const { h, onChange } = makeEngine();
    h.push([]);
    h.push(null as unknown as HistoryEntry[]);
    expect(h.canUndo()).toBe(false);
    expect(onChange).not.toHaveBeenCalled();
  });

  it("caps the undo depth at maxSteps (oldest dropped first)", () => {
    const { h, log } = makeEngine(3);
    for (let i = 0; i < 5; i += 1) {
      h.push([entry({ before: { i }, after: { i: i + 100 } })]);
    }
    let undone = 0;
    while (h.undo()) undone += 1;
    expect(undone).toBe(3);
    expect(log.map((w) => (w.value as { i: number }).i)).toEqual([4, 3, 2]);
  });

  it("notifies on push/undo/redo/clear so UI buttons can refresh", () => {
    const { h, onChange } = makeEngine();
    h.push([entry()]);
    h.undo();
    h.redo();
    h.clear();
    expect(onChange).toHaveBeenCalledTimes(4);
    expect(h.canUndo()).toBe(false);
    expect(h.canRedo()).toBe(false);
  });
});

describe("cloneValue", () => {
  it("deep-clones objects without aliasing and passes primitives through", () => {
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
