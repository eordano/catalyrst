import { describe, it, expect } from "vitest";
import {
  INITIAL_BOOT,
  bootReducer,
  bootOverlay,
  isEditorReady,
  isLiveEditing,
  type BootState,
  type BootEvent,
} from "./boot-machine";

const run = (events: BootEvent[], start: BootState = INITIAL_BOOT): BootState =>
  events.reduce(bootReducer, start);

describe("boot-machine", () => {
  it("narrates the boot overlay from idle through download, workers, compile and handshake", () => {
    expect(INITIAL_BOOT.phase).toBe("idle");
    expect(bootOverlay(INITIAL_BOOT).show).toBe(false);
    const booting = run([{ type: "viewport", src: "/_play/?x" }]);
    expect(booting.phase).toBe("booting");
    expect(bootOverlay(booting)).toMatchObject({ show: true, kind: "loading" });
    expect(bootOverlay(booting).text).toMatch(/loading/i);
    const dl = bootReducer(booting, { type: "progress", pct: 40 });
    expect(dl.stage).toBe("download");
    expect(bootOverlay(dl).text).toMatch(/download/i);
    expect(bootOverlay(dl).text).toContain("40%");
    const wk = bootReducer(dl, { type: "progress", pct: 92 });
    expect(wk.stage).toBe("workers");
    expect(bootOverlay(wk).text).toMatch(/workers/i);
    expect(bootOverlay(wk).text).toContain("92%");
    const explicit = bootReducer(dl, { type: "progress", pct: 82, stage: "compile" });
    expect(explicit.stage).toBe("compile");
    expect(bootOverlay(explicit).text).toMatch(/compil/i);
    expect(bootOverlay(explicit).text).toContain("82%");
    const handshaking = bootReducer(wk, { type: "progress", pct: 100 });
    expect(handshaking.phase).toBe("handshaking");
    expect(bootOverlay(handshaking).text).toMatch(/scene/i);
  });

  it("requires the scene handshake after engine startup and tears down on viewport(null)", () => {
    const live = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 100 },
      { type: "engine-ready" },
      { type: "scene-ready" },
    ]);
    expect(live.phase).toBe("ready");
    expect(isEditorReady(live)).toBe(true);
    expect(isLiveEditing(live)).toBe(true);
    expect(bootOverlay(live).show).toBe(false);
    const engineOnly = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 100 },
      { type: "engine-ready" },
    ]);
    expect(engineOnly.phase).toBe("handshaking");
    expect(isEditorReady(engineOnly)).toBe(false);
    expect(isLiveEditing(engineOnly)).toBe(false);
    expect(bootOverlay(engineOnly).show).toBe(true);
    expect(bootReducer(live, { type: "viewport", src: null })).toEqual(INITIAL_BOOT);
  });

  it("requires independent engine and scene facts before becoming ready", () => {
    const sceneFirst = run([
      { type: "viewport", src: "/_play" },
      { type: "scene-ready" },
    ]);
    expect(sceneFirst).toMatchObject({ phase: "handshaking", sceneReady: true, engineReady: false });
    expect(bootReducer(sceneFirst, { type: "engine-ready" })).toMatchObject({ phase: "ready", sceneReady: true, engineReady: true });
  });

  it("requires a real scene handshake to recover from a timeout", () => {
    const healthy = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 100 },
      { type: "engine-ready" },
      { type: "timeout" },
    ]);
    expect(healthy.phase).toBe("error");
    expect(bootOverlay(healthy).show).toBe(true);
    const timedOut = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 100 },
      { type: "timeout" },
    ]);
    expect(timedOut.phase).toBe("error");
    expect(bootOverlay(timedOut)).toMatchObject({ show: true, kind: "error" });
    expect(bootReducer(timedOut, { type: "engine-ready" }).phase).toBe("error");
    const sceneOnly = bootReducer(timedOut, { type: "scene-ready" });
    expect(sceneOnly).toMatchObject({ phase: "error", sceneReady: true, engineReady: false });
    const healed = bootReducer(healthy, { type: "scene-ready" });
    expect(healed.phase).toBe("ready");
    expect(bootOverlay(healed).show).toBe(false);
  });

  it("bus-reset waits for a fresh handshake", () => {
    const ready = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 100 },
      { type: "engine-ready" },
      { type: "scene-ready" },
    ]);
    const reset = bootReducer(ready, { type: "bus-reset" });
    expect(reset.phase).toBe("handshaking");
    expect(isLiveEditing(reset)).toBe(false);
    expect(bootOverlay(reset).show).toBe(true);
    const early = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 100 },
      { type: "bus-reset" },
    ]);
    expect(early.phase).toBe("handshaking");
    expect(early.sceneReady).toBe(false);
  });

  it("retry resets the previous engine state and an engine error remains visible", () => {
    const errored = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 100 },
      { type: "engine-error", reason: "boom" },
    ]);
    expect(errored.phase).toBe("error");
    expect(bootReducer(errored, { type: "retry" }).phase).toBe("booting");
    expect(bootReducer({ ...errored, engineReady: true }, { type: "retry" })).toMatchObject({ phase: "booting", engineReady: false, sceneReady: false });
    const ready = run([
      { type: "viewport", src: "/_play" },
      { type: "scene-ready" },
    ]);
    expect(bootReducer(ready, { type: "engine-error" }).phase).toBe("error");
    expect(bootReducer(ready, { type: "viewport", src: "/_play?project=other" })).toMatchObject({ phase: "booting", engineReady: false, sceneReady: false });
  });
});
