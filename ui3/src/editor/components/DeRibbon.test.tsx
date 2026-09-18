import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import DeRibbon from "./DeRibbon";
import { RIBBON_CHROME_IDS, RIBBON_DEFERRED, RIBBON_TABS } from "../ribbon-spec";

afterEach(cleanup);

describe("DeRibbon", () => {
  const tabNames = (): string[] => screen.getAllByRole("tab").map((t) => t.textContent ?? "");
  const selectedTab = (): string => screen.getByRole("tab", { selected: true }).textContent ?? "";
  const groupLabels = (container: HTMLElement): string[] =>
    Array.from(container.querySelectorAll(".rb-deck .rb-grouplabel")).map(
      (el) => el.textContent ?? "",
    );

  it("opens on Home as the first tab, and moves between them with the arrow, Home and End keys", () => {
    render(<DeRibbon />);
    expect(selectedTab()).toBe("Home");
    expect(tabNames()[0]).toBe("Home");
    const home = screen.getByRole("tab", { name: "Home" });
    fireEvent.keyDown(home, { key: "ArrowRight" });
    expect(selectedTab()).toBe("Insert");
    fireEvent.keyDown(home, { key: "ArrowLeft" });
    expect(selectedTab()).toBe("Home");
    fireEvent.keyDown(home, { key: "End" });
    expect(selectedTab()).toBe(tabNames().at(-1));
    fireEvent.keyDown(home, { key: "Home" });
    expect(selectedTab()).toBe("Home");
  });

  it("keeps Undo and Redo reachable from every tab without duplicate playback controls", () => {
    render(<DeRibbon commands={{ play: () => undefined }} tab="scene" busLive />);
    const qat = within(screen.getByRole("group", { name: "Quick access" }));
    for (const label of ["Undo", "Redo"]) expect(qat.getByLabelText(label)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Play" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Code tools" })).toBeNull();
  });

  it("offers Open from disk next to Save to disk on Scene & Publish, and says so when nothing is wired there", () => {
    const seen: string[] = [];
    const { rerender } = render(
      <DeRibbon
        tab="scene"
        commands={{ save: () => seen.push("save"), open: () => seen.push("open") }}
      />,
    );
    const project = within(screen.getByText("Project").parentElement as HTMLElement);
    expect(project.getByLabelText("Save to disk")).toBeTruthy();
    fireEvent.click(project.getByLabelText("Open from disk"));
    expect(seen).toEqual(["open"]);

    rerender(<DeRibbon tab="scene" />);
    expect(screen.getByText(RIBBON_TABS.find((t) => t.id === "scene")!.empty)).toBeTruthy();
  });

  it("explains why Make interactive is blocked for the scene root, and shows the code tools without an opt-in", () => {
    const hint = "The scene root can't be wired";
    const noop = () => undefined;
    render(
      <DeRibbon
        commands={{ "wire.quick": noop, code: noop }}
        tab="interact"
        busLive
        hasSelection={false}
        selectionHint={hint}
      />,
    );
    const wire = screen.getByLabelText("Make interactive") as HTMLButtonElement;
    expect(wire.disabled).toBe(true);
    expect(wire.title).toBe(hint);
    const before = tabNames();
    expect(screen.getByRole("button", { name: "Open code editor" })).toBeTruthy();
    expect(tabNames()).toEqual(before);
  });

  it("surfaces the selection as a group in Home, never as a tab", () => {
    const noop = () => undefined;
    const { container, rerender } = render(<DeRibbon commands={{ "item.focus": noop }} />);
    const before = tabNames();
    expect(groupLabels(container)).not.toContain("button_lights");
    rerender(
      <DeRibbon commands={{ "item.focus": noop }} hasSelection selectionLabel="button_lights" />,
    );
    expect(tabNames()).toEqual(before);
    expect(groupLabels(container)).toContain("button_lights");
  });

  it("omits commands that have no handler, disables only what the state blocks, and runs wired commands", () => {
    const noop = () => undefined;
    const spy = vi.fn();
    const { rerender } = render(<DeRibbon commands={{ undo: noop }} canUndo />);
    expect(screen.queryByRole("button", { name: "Duplicate" })).toBeNull();
    const qat = within(screen.getByRole("group", { name: "Quick access" }));
    expect((qat.getByLabelText("Undo") as HTMLButtonElement).disabled).toBe(false);

    rerender(<DeRibbon commands={{ duplicate: spy }} hasSelection={false} />);
    const dup = screen.getAllByRole("button", { name: "Duplicate" })[0] as HTMLButtonElement;
    expect(dup.disabled).toBe(true);
    expect(dup.title).toMatch(/select/i);
    expect(dup.title).not.toMatch(/not wired/i);

    rerender(<DeRibbon commands={{ duplicate: spy }} hasSelection />);
    const live = screen.getAllByRole("button", { name: "Duplicate" })[0] as HTMLButtonElement;
    expect(live.disabled).toBe(false);
    fireEvent.click(live);
    expect(spy).toHaveBeenCalledOnce();
  });

  it("shows snap state honestly and only the meters that were measured, including one past its limit", () => {
    const { rerender } = render(<DeRibbon />);
    expect(screen.getByText(/snap n\/a/i)).toBeTruthy();
    expect(screen.queryByText(/limits not measured/i)).toBeNull();
    expect(screen.queryByText(/\/\d+$/)).toBeNull();
    rerender(<DeRibbon snapLabel="snap 0.25 m" meters={[{ label: "entities", value: 41, limit: 317 }]} />);
    expect(screen.getByText("snap 0.25 m")).toBeTruthy();
    expect(screen.getByText("entities 41/317")).toBeTruthy();
    rerender(<DeRibbon meters={[{ label: "entities", value: 400, limit: 317 }]} />);
    expect(screen.getByText("entities 400/317")).toBeTruthy();
  });

  it("shows the camera pose in the numeric slot while nothing is selected, and the select-an-item hint until a pose arrives", () => {
    const { rerender } = render(
      <DeRibbon commands={{}} hasSelection={false} numeric={{ onCommit: () => {} }} />,
    );
    expect(screen.getByText("Select an item to type exact values.")).toBeTruthy();
    rerender(
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

  it("keeps the spec honest: pinned chrome never repeats inside a tab, and deferred commands never ship", () => {
    const shipped = new Set(RIBBON_TABS.flatMap((t) => t.groups.flatMap((g) => g.cmds.map((c) => c.id))));
    for (const id of RIBBON_CHROME_IDS) {
      expect(shipped.has(id), `${id} is pinned chrome AND a tab command`).toBe(false);
    }
    expect(RIBBON_DEFERRED.length).toBeGreaterThan(0);
    for (const d of RIBBON_DEFERRED) expect(shipped.has(d.id)).toBe(false);
  });
});
