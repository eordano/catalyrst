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
  it("starts idle with no overlay", () => {
    expect(INITIAL_BOOT.phase).toBe("idle");
    expect(bootOverlay(INITIAL_BOOT).show).toBe(false);
  });

  it("idle -> booting when a viewport mounts", () => {
    const s = run([{ type: "viewport", src: "/_play/?x" }]);
    expect(s.phase).toBe("booting");
    expect(bootOverlay(s)).toMatchObject({ show: true, kind: "loading" });
  });

  it("progress 100 moves booting -> handshaking (Starting scene…)", () => {
    const s = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 40 },
      { type: "progress", pct: 100 },
    ]);
    expect(s.phase).toBe("handshaking");
    expect(bootOverlay(s).text).toBe("Starting scene…");
  });

  it("bus scene-ready => ready + live editing, overlay gone", () => {
    const s = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 100 },
      { type: "scene-ready" },
    ]);
    expect(s.phase).toBe("ready");
    expect(isEditorReady(s)).toBe(true);
    expect(isLiveEditing(s)).toBe(true);
    expect(bootOverlay(s).show).toBe(false);
  });

  it("engine-ready alone (missed handshake) => ready, overlay dismissed", () => {
    const s = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 100 },
      { type: "engine-ready" },
    ]);
    expect(s.phase).toBe("ready");
    expect(isEditorReady(s)).toBe(true);
    expect(isLiveEditing(s)).toBe(false);
    expect(bootOverlay(s).show).toBe(false);
  });

  it("timeout with engine already up self-heals to ready (no dead end)", () => {
    const s = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 100 },
      { type: "engine-ready" },
      { type: "timeout" },
    ]);
    expect(s.phase).toBe("ready");
    expect(bootOverlay(s).show).toBe(false);
  });

  it("timeout without any ready signal -> soft error, then engine-ready clears it", () => {
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

  it("bus-reset after ready keeps the engine interactive but drops live editing", () => {
    const ready = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 100 },
      { type: "scene-ready" },
    ]);
    const reset = bootReducer(ready, { type: "bus-reset" });
    expect(reset.phase).toBe("ready");
    expect(isLiveEditing(reset)).toBe(false);
    expect(bootOverlay(reset).show).toBe(false);
  });

  it("bus-reset before engine-ready falls back to handshaking (re-blocks)", () => {
    const s = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 100 },
      { type: "bus-reset" },
    ]);
    expect(s.phase).toBe("handshaking");
    expect(s.sceneReady).toBe(false);
  });

  it("retry from error re-kicks to handshaking when the engine is up", () => {
    const errored = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 100 },
      { type: "engine-error", reason: "boom" },
    ]);
    expect(errored.phase).toBe("error");
    const retried = bootReducer(errored, { type: "retry" });
    expect(retried.phase).toBe("booting");
    const withEngine = bootReducer({ ...errored, engineReady: true }, { type: "retry" });
    expect(withEngine.phase).toBe("handshaking");
  });

  it("viewport(null) tears everything back down to idle", () => {
    const ready = run([
      { type: "viewport", src: "/_play" },
      { type: "scene-ready" },
    ]);
    const gone = bootReducer(ready, { type: "viewport", src: null });
    expect(gone).toEqual(INITIAL_BOOT);
  });

  it("engine-error while already interactive is ignored (no flicker to error)", () => {
    const ready = run([
      { type: "viewport", src: "/_play" },
      { type: "scene-ready" },
    ]);
    const s = bootReducer(ready, { type: "engine-error" });
    expect(s.phase).toBe("ready");
  });

  it("progress without explicit stage derives it from percent", () => {
    const dl = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 40 },
    ]);
    expect(dl.stage).toBe("download");
    expect(bootOverlay(dl).text).toBe("Downloading engine… 40%");
    const wk = bootReducer(dl, { type: "progress", pct: 92 });
    expect(wk.stage).toBe("workers");
    expect(bootOverlay(wk).text).toBe("Starting workers… 92%");
  });

  it("explicit engine stage wins over the percent heuristic", () => {
    const s = run([
      { type: "viewport", src: "/_play" },
      { type: "progress", pct: 82, stage: "compile" },
    ]);
    expect(s.stage).toBe("compile");
    expect(bootOverlay(s).text).toBe("Compiling engine… 82%");
  });

  it("no progress yet keeps the generic loading text", () => {
    const s = run([{ type: "viewport", src: "/_play" }]);
    expect(bootOverlay(s).text).toBe("Loading scene editor…");
  });
});
