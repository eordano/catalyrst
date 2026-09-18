import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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
    if (data.msg.type === "rpc" && data.msg.method === "exportComposite") queueMicrotask(() => deliver({ type: "rpc-reply", id: data.msg.id, ok: true, result: '{"version":1,"components":[]}' }));
  }
  close(): void {}
}

const sent = (type: string) => FakeChannel.posted.filter((e) => e.msg.type === type).map((e) => e.msg);

function deliver(msg: Record<string, unknown>) {
  act(() => {
    for (const ch of FakeChannel.instances) ch.onmessage?.({ data: { to: "page", msg } });
  });
}

async function bootLive(selected: string[], active: string | null) {
  render(<DeWorkspace title="Guard" viewportSrc="https://catalyst.example.com/_play/?editorSession=00000000-0000-4000-8000-000000000001&x=1" />);
  Object.assign((screen.getByTitle("Scene viewport") as HTMLIFrameElement).contentWindow!, { dclEngineReady: true });
  deliver({
    type: "scene-ready",
    bridge: 8,
    scene: { hash: "scene-a", title: "Scene", parcels: [], isPortable: false, isBroken: false, isBlocked: false, isSuper: false, sdkVersion: "7" },
    frozen: true,
    tool: "translate",
    orientGlobal: false,
    pivotEach: false,
    selected,
    active,
  });
  await waitFor(() => expect(screen.getByRole("button", { name: "Run the scene" })).not.toBeDisabled());
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
  it("does not clear a new selection while deletion finishes saving undo history", async () => {
    await bootLive(["512"], "512");
    deliver({ type: "entities", entities: [{ id: "512", name: "Palm", parent: "0" }, { id: "513", name: "Fern", parent: "0" }] });
    deliver({ type: "selection", selected: ["512"], active: "512", components: { "512": PALM } });
    fireEvent.keyDown(window, { key: "Delete" });
    await waitFor(() => expect(sent("rpc").some(msg => msg.method === "removeEntities")).toBe(true));
    const remove = sent("rpc").find(msg => msg.method === "removeEntities")!;
    deliver({ type: "selection", selected: [], active: null, components: {} });
    deliver({ type: "selection", selected: ["513"], active: "513", components: { "513": PALM } });
    deliver({ type: "rpc-reply", id: remove.id, ok: true, result: ["512"] });
    await waitFor(() => expect(screen.getByLabelText("Undo").hasAttribute("disabled")).toBe(false));
    expect(sent("set-selection").some(message => Array.isArray(message.selected) && !message.selected.length)).toBe(false);
    expect(screen.getByText("#513")).toBeTruthy();
  });

  it("Delete and Ctrl+D with the root selected send nothing to the scene", async () => {
    await bootLive(["0"], "0");
    expect(FakeChannel.instances.length).toBeGreaterThan(0);
    deliver({ type: "selection", selected: ["0"], active: "0", components: { "0": { Name: { value: "Scene" } } } });
    fireEvent.keyDown(window, { key: "Delete" });
    fireEvent.keyDown(window, { key: "Backspace" });
    fireEvent.keyDown(window, { key: "d", ctrlKey: true });
    expect(sent("entity-deleted")).toEqual([]);
    expect(sent("add-entity")).toEqual([]);
    expect(sent("rpc").filter(msg => msg.method === "copyEntities" || msg.method === "pasteEntities")).toEqual([]);
    const del = screen.getByLabelText("Delete") as HTMLButtonElement;
    expect(del.disabled).toBe(true);
  });

  it("a root + item multi-selection deletes only the item, and a placed item still deletes and duplicates", async () => {
    await bootLive(["0", "512"], "512");
    deliver({ type: "entities", entities: [{ id: "512", name: "Palm", parent: "0" }] });
    deliver({ type: "selection", selected: ["0", "512"], active: "512", components: { "512": PALM } });
    fireEvent.keyDown(window, { key: "Delete" });
    await waitFor(() => expect(sent("rpc").find(msg => msg.method === "removeEntities")?.args).toEqual([["512"]]));
    const remove = sent("rpc").find(msg => msg.method === "removeEntities")!;
    deliver({ type: "rpc-reply", id: remove.id, ok: true, result: ["512"] });
    await waitFor(() => expect(screen.getByLabelText("Undo").hasAttribute("disabled")).toBe(false));

    deliver({ type: "selection", selected: ["512"], active: "512", components: { "512": PALM } });
    fireEvent.keyDown(window, { key: "d", ctrlKey: true });
    const copy = sent("rpc").find(msg => msg.method === "copyEntities")!;
    expect(copy.args).toEqual([["512"]]);
    expect(sent("rpc").filter(msg => msg.method === "pasteEntities")).toEqual([]);
    const subtree = { version: 1, roots: ["512"], composite: "{\"version\":1,\"components\":[]}" };
    deliver({ type: "rpc-reply", id: copy.id, ok: true, result: subtree });
    await waitFor(() => expect(sent("rpc").find(msg => msg.method === "pasteEntities")?.args).toEqual([subtree, "0"]));
    const paste = sent("rpc").find(msg => msg.method === "pasteEntities")!;
    deliver({ type: "rpc-reply", id: paste.id, ok: true, result: ["600"] });
    await act(async () => {});
    fireEvent.keyDown(window, { key: "Delete" });
    await waitFor(() => expect(sent("rpc").filter(msg => msg.method === "removeEntities")).toHaveLength(2));
  });
});
