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
    expect(bootOverlay(booting).text).toBe("Loading scene editor\u{2026}");
    const dl = bootReducer(booting, { type: "progress", pct: 40 });
    expect(dl.stage).toBe("download");
    expect(bootOverlay(dl).text).toBe("Downloading engine\u{2026} 40%");
    const wk = bootReducer(dl, { type: "progress", pct: 92 });
    expect(wk.stage).toBe("workers");
    expect(bootOverlay(wk).text).toBe("Starting workers\u{2026} 92%");
    const explicit = bootReducer(dl, { type: "progress", pct: 82, stage: "compile" });
    expect(explicit.stage).toBe("compile");
    expect(bootOverlay(explicit).text).toBe("Compiling engine\u{2026} 82%");
    const handshaking = bootReducer(wk, { type: "progress", pct: 100 });
    expect(handshaking.phase).toBe("handshaking");
    expect(bootOverlay(handshaking).text).toBe("Starting scene\u{2026}");
  });

  it("becomes ready on scene-ready (live editing) or engine-ready alone, and tears down on viewport(null)", () => {
    const live = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 100 },
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
    expect(engineOnly.phase).toBe("ready");
    expect(isEditorReady(engineOnly)).toBe(true);
    expect(isLiveEditing(engineOnly)).toBe(false);
    expect(bootOverlay(engineOnly).show).toBe(false);
    expect(bootReducer(live, { type: "viewport", src: null })).toEqual(INITIAL_BOOT);
  });

  it("times out into a soft error that engine-ready heals, and never dead-ends when the engine is already up", () => {
    const healthy = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 100 },
      { type: "engine-ready" },
      { type: "timeout" },
    ]);
    expect(healthy.phase).toBe("ready");
    expect(bootOverlay(healthy).show).toBe(false);
    const timedOut = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 100 },
      { type: "timeout" },
    ]);
    expect(timedOut.phase).toBe("error");
    expect(bootOverlay(timedOut)).toMatchObject({ show: true, kind: "error" });
    const healed = bootReducer(timedOut, { type: "engine-ready" });
    expect(healed.phase).toBe("ready");
    expect(bootOverlay(healed).show).toBe(false);
  });

  it("bus-reset drops live editing after ready but re-blocks before the engine is up", () => {
    const ready = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 100 },
      { type: "scene-ready" },
    ]);
    const reset = bootReducer(ready, { type: "bus-reset" });
    expect(reset.phase).toBe("ready");
    expect(isLiveEditing(reset)).toBe(false);
    expect(bootOverlay(reset).show).toBe(false);
    const early = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 100 },
      { type: "bus-reset" },
    ]);
    expect(early.phase).toBe("handshaking");
    expect(early.sceneReady).toBe(false);
  });

  it("retry re-kicks from error, and an engine-error while interactive is ignored", () => {
    const errored = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 100 },
      { type: "engine-error", reason: "boom" },
    ]);
    expect(errored.phase).toBe("error");
    expect(bootReducer(errored, { type: "retry" }).phase).toBe("booting");
    expect(bootReducer({ ...errored, engineReady: true }, { type: "retry" }).phase).toBe("handshaking");
    const ready = run([
      { type: "viewport", src: "/_play" },
      { type: "scene-ready" },
    ]);
    expect(bootReducer(ready, { type: "engine-error" }).phase).toBe("ready");
  });
});
