import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { diffSnapshots, type DebugSnapshot } from "../debugger";
import DeDebugPanel, { DEBUG_ROW_CAP } from "./DeDebugPanel";

afterEach(cleanup);

const xf = (z: number) => ({
  position: { x: 8, y: 0, z },
  rotation: { x: 0, y: 0, z: 0, w: 1 },
  scale: { x: 1, y: 1, z: 1 },
});

const prev: DebugSnapshot = {
  "512": { Transform: xf(13.4), GltfContainer: { src: "creep.glb" } },
  "513": { Transform: xf(2) },
};
const next: DebugSnapshot = {
  "512": { Transform: xf(13.187), GltfContainer: { src: "creep.glb" } },
  "600": { TextShape: { text: "TOWER DEFENSE" } },
};

describe("DeDebugPanel", () => {
  it("shows the engine tick, changed components with old→new values, and badges", () => {
    render(
      <DeDebugPanel
        tick={456}
        entries={diffSnapshots(prev, next)}
        lastStepCount={10}
        totalEntities={2}
        unchangedEntities={0}
        names={(id) => (id === "512" ? "Creep Spider A" : `Entity ${id}`)}
      />,
    );
    expect(screen.getByText("Engine tick 456")).toBeTruthy();
    expect(screen.getByText("Creep Spider A")).toBeTruthy();
    expect(screen.getByText("position.z")).toBeTruthy();
    expect(screen.getByText("13.4")).toBeTruthy();
    expect(screen.getByText("13.187")).toBeTruthy();
    expect(screen.getByText("new")).toBeTruthy();
    expect(screen.getByText("gone")).toBeTruthy();
    expect(screen.getByText(/\+10 ticks/)).toBeTruthy();
    expect(screen.queryByText("GltfContainer")).toBeNull();
    expect(screen.getByText("1 unchanged")).toBeTruthy();
  });

  it("shows the baseline hint before the first step and step buttons fire", () => {
    const onStep = vi.fn();
    render(<DeDebugPanel tick={100} entries={null} onStep={onStep} />);
    expect(screen.getByText(/Baseline captured at engine tick 100/)).toBeTruthy();
    fireEvent.click(screen.getByTitle("Advance 1 tick (.)"));
    fireEvent.click(screen.getByTitle("Advance 10 ticks"));
    fireEvent.click(screen.getByTitle("Advance 60 ticks"));
    expect(onStep.mock.calls.map((c) => c[0])).toEqual([1, 10, 60]);
  });

  it("disables stepping while a step is in flight", () => {
    const onStep = vi.fn();
    render(<DeDebugPanel tick={100} entries={null} stepping onStep={onStep} />);
    const btn = screen.getByTitle("Advance 1 tick (.)") as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
    fireEvent.click(btn);
    expect(onStep).not.toHaveBeenCalled();
  });

  it("caps rendered rows and reveals the rest via Show all", () => {
    const big: ReturnType<typeof diffSnapshots> = Array.from({ length: 75 }, (_, i) => ({
      id: String(512 + i),
      kind: "changed" as const,
      comps: [{ name: "Transform", kind: "changed" as const, changes: [] }],
      unchanged: 0,
    }));
    render(<DeDebugPanel tick={1} entries={big} totalEntities={80} unchangedEntities={5} />);
    expect(document.querySelectorAll(".eui-dbg-entity")).toHaveLength(DEBUG_ROW_CAP);
    fireEvent.click(screen.getByText(/Show all 75 changed entities/));
    expect(document.querySelectorAll(".eui-dbg-entity")).toHaveLength(75);
  });

  it("clicking an entity selects it in the editor (gone entities are not selectable)", () => {
    const onSelect = vi.fn();
    render(
      <DeDebugPanel
        tick={1}
        entries={diffSnapshots(prev, next)}
        onSelect={onSelect}
        names={(id) => `E${id}`}
      />,
    );
    fireEvent.click(screen.getByText("E512"));
    expect(onSelect).toHaveBeenCalledWith("512");
    fireEvent.click(screen.getByText("E513"));
    expect(onSelect).toHaveBeenCalledTimes(1);
  });

  it("renders systems with executed indicators, or the honest non-template note", () => {
    const { unmount } = render(
      <DeDebugPanel
        tick={1}
        entries={null}
        systems={{
          game: "tower-defense",
          rows: [
            { name: "march creeps", ran: true, runs: 42 },
            { name: "pad flash timer", ran: false, runs: 3 },
          ],
          handlers: 2,
          harnessTick: 42,
        }}
      />,
    );
    expect(screen.getByText("tower-defense")).toBeTruthy();
    expect(screen.getByText("march creeps")).toBeTruthy();
    expect(screen.getByText(/ran · 42/)).toBeTruthy();
    expect(screen.getByText(/idle · 3/)).toBeTruthy();
    expect(screen.getByText(/2 pointer handlers wired/)).toBeTruthy();
    unmount();

    render(<DeDebugPanel tick={1} entries={null} systems={null} />);
    expect(screen.getByText(/No system telemetry from this scene/)).toBeTruthy();
    expect(screen.getByText(/aren’t introspectable/)).toBeTruthy();
  });

  it("reports an error state and the barrier timeout honestly", () => {
    const { unmount } = render(
      <DeDebugPanel tick={null} entries={null} error="Engine console unavailable" />,
    );
    expect(screen.getByRole("alert").textContent).toContain("Engine console unavailable");
    unmount();

    render(<DeDebugPanel tick={9} entries={[]} totalEntities={4} timedOut />);
    expect(screen.getByText(/step barrier timed out — partial/)).toBeTruthy();
    expect(screen.getByText(/no component changes \(4 entities\)/)).toBeTruthy();
  });
});
