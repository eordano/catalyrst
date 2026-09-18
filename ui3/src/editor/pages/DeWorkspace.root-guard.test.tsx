import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import DeWorkspace from "./DeWorkspace";

type Envelope = { to: string; msg: Record<string, unknown> };

class FakeChannel {
  static instances: FakeChannel[] = [];
  static posted: Envelope[] = [];
  onmessage: ((ev: { data: Envelope }) => void) | null = null;
  constructor(public name: string) {
    FakeChannel.instances.push(this);
  }
  postMessage(data: Envelope): void {
    FakeChannel.posted.push(data);
  }
  close(): void {}
}

const sent = (type: string) => FakeChannel.posted.filter((e) => e.msg.type === type).map((e) => e.msg);

function deliver(msg: Record<string, unknown>) {
  act(() => {
    for (const ch of FakeChannel.instances) ch.onmessage?.({ data: { to: "page", msg } });
  });
}

function bootLive(selected: string[], active: string | null) {
  render(<DeWorkspace title="Guard" viewportSrc="https://catalyst.example.com/_play/?x=1" />);
  deliver({
    type: "scene-ready",
    bridge: 8,
    scene: null,
    frozen: true,
    tool: "translate",
    orientGlobal: false,
    pivotEach: false,
    selected,
    active,
  });
}

const PALM = { Transform: { position: { x: 1, y: 0, z: 1 }, parent: 0 }, Name: { value: "Palm" } };

beforeEach(() => {
  FakeChannel.instances = [];
  FakeChannel.posted = [];
  vi.stubGlobal("BroadcastChannel", FakeChannel);
  window.localStorage.setItem("eui-camera-hint-dismissed", "1");
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  window.localStorage.clear();
});

describe("scene root is never deleted or duplicated", () => {
  it("Delete and Ctrl+D with the root selected send nothing to the scene", () => {
    bootLive(["0"], "0");
    expect(FakeChannel.instances.length).toBeGreaterThan(0);
    deliver({ type: "selection", selected: ["0"], active: "0", components: { "0": { Name: { value: "Scene" } } } });
    fireEvent.keyDown(window, { key: "Delete" });
    fireEvent.keyDown(window, { key: "Backspace" });
    fireEvent.keyDown(window, { key: "d", ctrlKey: true });
    expect(sent("entity-deleted")).toEqual([]);
    expect(sent("add-entity")).toEqual([]);
    const del = screen.getByLabelText("Delete") as HTMLButtonElement;
    expect(del.disabled).toBe(true);
  });

  it("a root + item multi-selection deletes only the item, and a placed item still deletes and duplicates", () => {
    bootLive(["0", "512"], "512");
    deliver({ type: "entities", entities: [{ id: "512", name: "Palm", parent: "0" }] });
    deliver({ type: "selection", selected: ["0", "512"], active: "512", components: { "512": PALM } });
    fireEvent.keyDown(window, { key: "Delete" });
    expect(sent("entity-deleted")).toEqual([{ type: "entity-deleted", entity: "512", recursive: true }]);

    deliver({ type: "selection", selected: ["512"], active: "512", components: { "512": PALM } });
    fireEvent.keyDown(window, { key: "d", ctrlKey: true });
    expect(sent("add-entity")).toEqual([
      { type: "add-entity", name: "Palm copy", parent: 0, components: { Transform: PALM.Transform } },
    ]);
    fireEvent.keyDown(window, { key: "Delete" });
    expect(sent("entity-deleted")).toHaveLength(2);
  });
});
