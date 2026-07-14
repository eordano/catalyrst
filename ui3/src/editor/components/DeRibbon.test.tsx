import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import DeRibbon from "./DeRibbon";
import { RIBBON_CHROME_IDS, RIBBON_DEFERRED, RIBBON_TABS } from "../ribbon-spec";
import chromeCss from "../frames/dcleditorchrome.css?raw";

afterEach(cleanup);

// These assertions are the study's conclusions, not preferences. Each one
// corresponds to a finding in the dclux editor UX observation study: if a
// future refactor moves Preview behind a tab or lets a tab reorder itself, the
// design has been undone and this should go red.
describe("DeRibbon", () => {
  const tabNames = (): string[] => screen.getAllByRole("tab").map((t) => t.textContent ?? "");
  const groupLabels = (container: HTMLElement): string[] =>
    Array.from(container.querySelectorAll(".rb-deck .rb-grouplabel")).map(
      (el) => el.textContent ?? "",
    );

  it("opens on Home, because the arrange loop is where sessions live", () => {
    render(<DeRibbon />);
    expect(screen.getByRole("tab", { selected: true }).textContent).toBe("Home");
  });

  it("ranks the static tabs by observed frequency", () => {
    render(<DeRibbon />);
    expect(tabNames()).toEqual(["Home", "Insert", "Interact", "Scene & Publish"]);
  });

  // TEST is 275 events across 44 of 48 videos and terminates every loop, so the
  // in-editor run must never be a tab away. It is pinned as "Play"; Publish lives
  // on the app bar with its target split, not duplicated here.
  it("keeps Undo, Redo and Play reachable from every tab", () => {
    const seen: string[] = [];
    render(<DeRibbon commands={{ play: () => seen.push("play") }} tab="scene" busLive />);
    const qat = within(screen.getByRole("group", { name: "Quick access" }));
    const always = within(screen.getByRole("group", { name: "Always available" }));
    for (const label of ["Undo", "Redo"]) expect(qat.getByLabelText(label)).toBeTruthy();
    expect(always.getByLabelText("Play")).toBeTruthy();
    expect(always.queryByLabelText("Publish")).toBeNull();
    fireEvent.click(always.getByLabelText("Play"));
    expect(seen).toEqual(["play"]);
  });

  it("shows Play disabled, not hidden, while the engine is connecting", () => {
    const seen: string[] = [];
    render(<DeRibbon commands={{ play: () => seen.push("play") }} />);
    const always = within(screen.getByRole("group", { name: "Always available" }));
    const play = always.getByLabelText("Play") as HTMLButtonElement;
    expect(play.disabled).toBe(true);
    fireEvent.click(play);
    expect(seen).toEqual([]);
  });

  // A persona split is not a surface split: the code tools opt in as a group,
  // and the tab strip is byte-identical either way.
  it("hides the code tools until they are opted into, without moving a tab", () => {
    const noop = () => undefined;
    const { rerender } = render(<DeRibbon commands={{ code: noop }} tab="interact" />);
    const before = tabNames();
    expect(screen.queryByRole("button", { name: "Open code editor" })).toBeNull();
    rerender(<DeRibbon commands={{ code: noop }} tab="interact" showDeveloper />);
    expect(screen.getByRole("button", { name: "Open code editor" })).toBeTruthy();
    expect(tabNames()).toEqual(before);
  });

  it("surfaces the selection as the last group in Home, never as a tab", () => {
    const noop = () => undefined;
    const { container, rerender } = render(<DeRibbon commands={{ "item.focus": noop }} />);
    const before = tabNames();
    expect(groupLabels(container)).not.toContain("button_lights");
    rerender(
      <DeRibbon commands={{ "item.focus": noop }} hasSelection selectionLabel="button_lights" />,
    );
    expect(tabNames()).toEqual(before);
    const labels = groupLabels(container);
    expect(labels).toContain("button_lights");
    expect(labels[labels.length - 1]).toBe("button_lights");
  });

  // "Dishonest failure surfaces" is one of the eight complaint classes. A
  // capability the editor does not have is not rendered at all; a capability
  // blocked by the current state is disabled and names the state.
  it("omits commands that have no handler, and disables only what the state blocks", () => {
    const noop = () => undefined;
    const { rerender } = render(<DeRibbon commands={{ undo: noop }} canUndo />);
    expect(screen.queryByRole("button", { name: "Duplicate" })).toBeNull();
    const qat = within(screen.getByRole("group", { name: "Quick access" }));
    expect((qat.getByLabelText("Undo") as HTMLButtonElement).disabled).toBe(false);

    rerender(<DeRibbon commands={{ duplicate: noop }} hasSelection={false} />);
    const dup = screen.getAllByRole("button", { name: "Duplicate" })[0] as HTMLButtonElement;
    expect(dup.disabled).toBe(true);
    expect(dup.title).toMatch(/select/i);
    expect(dup.title).not.toMatch(/not wired/i);

    rerender(<DeRibbon commands={{ duplicate: noop }} hasSelection />);
    expect((screen.getAllByRole("button", { name: "Duplicate" })[0] as HTMLButtonElement).disabled).toBe(
      false,
    );
  });

  it("runs a wired command", () => {
    const spy = vi.fn();
    render(<DeRibbon commands={{ duplicate: spy }} hasSelection />);
    fireEvent.click(screen.getAllByRole("button", { name: "Duplicate" })[0]!);
    expect(spy).toHaveBeenCalledOnce();
  });

  // "Invisible mode state": snap must be readable at all times, and nothing may
  // change it as a side effect.
  it("always shows snap state in the status bar, honestly when unknown", () => {
    const { rerender } = render(<DeRibbon />);
    expect(screen.getByText(/snap n\/a/i)).toBeTruthy();
    rerender(<DeRibbon snapLabel="snap 0.25 m" />);
    expect(screen.getByText("snap 0.25 m")).toBeTruthy();
  });

  it("shows no meter chip at all when nothing is measured", () => {
    const { rerender } = render(<DeRibbon />);
    expect(screen.queryByText(/limits not measured/i)).toBeNull();
    expect(screen.queryByText(/\/\d+$/)).toBeNull();
    rerender(<DeRibbon meters={[{ label: "entities", value: 41, limit: 317 }]} />);
    expect(screen.getByText("entities 41/317")).toBeTruthy();
  });

  it("marks a meter over its limit", () => {
    render(<DeRibbon meters={[{ label: "entities", value: 400, limit: 317 }]} />);
    expect(screen.getByText("entities 400/317").className).toMatch(/over/);
  });

  it("moves between tabs with the arrow keys", () => {
    render(<DeRibbon />);
    const home = screen.getByRole("tab", { name: "Home" });
    fireEvent.keyDown(home, { key: "ArrowRight" });
    expect(screen.getByRole("tab", { selected: true }).textContent).toBe("Insert");
    fireEvent.keyDown(home, { key: "ArrowLeft" });
    expect(screen.getByRole("tab", { selected: true }).textContent).toBe("Home");
  });

  it("shows the camera pose in the numeric slot while nothing is selected", () => {
    render(
      <DeRibbon
        commands={{}}
        hasSelection={false}
        numeric={{ onCommit: () => {} }}
        cameraPose={{ x: 12.34, y: 2, z: -7.06, yaw: 91.2, pitch: -14.49 }}
      />,
    );
    const readout = screen.getByRole("status", { name: "Camera position and orientation" });
    expect(readout.textContent).toContain("12.3");
    expect(readout.textContent).toContain("-7.1");
    expect(readout.textContent).toContain("91.2\u{00B0}");
    expect(readout.textContent).toContain("-14.5\u{00B0}");
    expect(screen.getByText("Camera")).toBeTruthy();
    expect(screen.queryByText("Select an item to type exact values.")).toBeNull();
  });

  it("keeps the select-an-item hint when no camera pose has arrived", () => {
    render(<DeRibbon commands={{}} hasSelection={false} numeric={{ onCommit: () => {} }} />);
    expect(screen.getByText("Select an item to type exact values.")).toBeTruthy();
  });

  it("never repeats a pinned chrome command inside a tab", () => {
    const tabIds = new Set(
      RIBBON_TABS.flatMap((t) => t.groups.flatMap((g) => g.cmds.map((c) => c.id))),
    );
    for (const id of RIBBON_CHROME_IDS) {
      expect(tabIds.has(id), `${id} is pinned chrome AND a tab command`).toBe(false);
    }
  });

  it("carries its evidence: every tab states why it exists at that rank", () => {
    for (const t of RIBBON_TABS) {
      expect(t.why.length).toBeGreaterThan(40);
    }
  });

  // The study inventory survives as documentation rather than as dead buttons,
  // and a command may never be both shipped and deferred.
  it("gives every deferred command a reason, and never ships it too", () => {
    const shipped = new Set(RIBBON_TABS.flatMap((t) => t.groups.flatMap((g) => g.cmds.map((c) => c.id))));
    expect(RIBBON_DEFERRED.length).toBeGreaterThan(0);
    for (const d of RIBBON_DEFERRED) {
      expect(d.why.length).toBeGreaterThan(20);
      expect(shipped.has(d.id)).toBe(false);
    }
  });

  // The four defects of the prototype's numeric field, in one test.
  it("types a decimal, commits on Enter and on blur, and reverts on Escape", () => {
    const spy = vi.fn();
    render(<DeRibbon hasSelection numeric={{ position: { x: 1, y: 2, z: 3 }, onCommit: spy }} />);
    const x = screen.getByLabelText("Position X") as HTMLInputElement;

    fireEvent.change(x, { target: { value: "" } });
    expect(x.value).toBe("");

    fireEvent.change(x, { target: { value: "-3.5" } });
    fireEvent.keyDown(x, { key: "Enter" });
    expect(spy).toHaveBeenCalledWith("position", "x", -3.5);

    fireEvent.change(x, { target: { value: "9" } });
    fireEvent.keyDown(x, { key: "Escape" });
    expect(x.value).toBe("1");

    spy.mockClear();
    fireEvent.change(x, { target: { value: "7.75" } });
    fireEvent.blur(x);
    expect(spy).toHaveBeenCalledWith("position", "x", 7.75);
  });

  it("says so when a tab has nothing wired, instead of showing dead chips", () => {
    render(<DeRibbon tab="scene" />);
    expect(screen.getByText(RIBBON_TABS.find((t) => t.id === "scene")!.empty)).toBeTruthy();
  });

  // The bug this rebuild exists to fix: an in-flow block paints below every
  // positioned descendant, so the band needs its own layer and hit-testing.
  // The live proof is a browser elementFromPoint; this guards the rule itself.
  it("keeps the ribbon on its own layer above the engine iframe", () => {
    const rule = /\.eui-root\s*>\s*\.rb\s*\{([^}]*)\}/.exec(chromeCss);
    expect(rule).not.toBeNull();
    const body = rule![1]!;
    expect(body).toMatch(/position:\s*absolute/);
    expect(body).toMatch(/z-index:\s*\d+/);
    expect(body).toMatch(/pointer-events:\s*auto/);
    expect(chromeCss).toMatch(/--eui-top-inset:\s*calc\(var\(--eui-chrome-h\)/);
  });
  // These three shipped as defects and were fixed in 4df12bfe9. Each regresses
  // silently on a rename or a height tweak, so each gets a guard.
  it("names its toggles rb-toggle, so the global .toggle pill cannot claim them", () => {
    render(<DeRibbon commands={{ snap: () => undefined }} pressed={{ snap: false }} />);
    const snap = screen.getByRole("button", { name: "Snap" });
    expect(snap.className).toContain("rb-toggle");
    // atoms/toggle.css declares a global unnamespaced `.toggle { width: 42px }`
    // which clamped these to a pill and painted the label outside the button.
    expect(snap.className.split(/\s+/)).not.toContain("toggle");
  });

  it("moves to the first and last tab with Home and End", () => {
    render(<DeRibbon />);
    const home = screen.getByRole("tab", { name: "Home" });
    fireEvent.keyDown(home, { key: "End" });
    const last = screen.getAllByRole("tab").slice(-1)[0];
    expect(screen.getByRole("tab", { selected: true }).textContent).toBe(last?.textContent);
    fireEvent.keyDown(home, { key: "Home" });
    expect(screen.getByRole("tab", { selected: true }).textContent).toBe("Home");
  });

  it("points aria-controls only at a panel that exists", () => {
    const { container } = render(<DeRibbon />);
    for (const tab of screen.getAllByRole("tab")) {
      const target = tab.getAttribute("aria-controls");
      if (target === null) continue;
      expect(container.ownerDocument.getElementById(target)).not.toBeNull();
    }
  });
});
