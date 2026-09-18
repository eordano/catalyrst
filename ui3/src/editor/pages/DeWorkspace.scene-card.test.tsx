import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import DeWorkspace from "./DeWorkspace";
import { countPlaced, formatSpawn, readSceneMeta } from "../components/DeSceneCard";

afterEach(cleanup);

const TREE = [
  { id: "0", name: "Beach House", children: [{ id: "512", name: "Palm" }, { id: "513", name: "Chair" }] },
];

describe("scene root selection shows scene properties", () => {
  it("renders the scene card when the root is active and the entity inspector for a placed item", () => {
    const root = render(
      <DeWorkspace
        title="Beach House"
        tree={TREE}
        inspector={{ id: "0", name: "Beach House" }}
        sceneInfo={{ base: "12,-4", parcels: ["12,-4", "13,-4"], template: "castaway-2048" }}
      />,
    );
    const card = within(screen.getByRole("region", { name: "Scene properties" }));
    expect(screen.queryByRole("region", { name: "Entity components" })).toBeNull();
    expect(card.getByText(/2 parcels: 12,-4\s+13,-4/)).toBeTruthy();
    expect(card.getByText("12,-4")).toBeTruthy();
    expect(card.getByText(/the engine picks a default/)).toBeTruthy();
    expect(card.getByText("2")).toBeTruthy();
    expect(card.getByText("castaway-2048")).toBeTruthy();
    root.unmount();

    render(
      <DeWorkspace title="Beach House" tree={TREE} inspector={{ id: "512", name: "Palm" }} />,
    );
    expect(screen.getByRole("region", { name: "Entity components" })).toBeTruthy();
    expect(screen.queryByRole("region", { name: "Scene properties" })).toBeNull();
  });

  it("reads name and spawn points from the metadata component and counts placed items excluding the root", () => {
    const meta = readSceneMeta({
      name: "Beach House",
      spawnPoints: [{ name: "dock", default: true, position: { x: 8, y: [0, 1], z: 8 } }],
    });
    expect(meta.name).toBe("Beach House");
    expect(meta.spawnPoints.map(formatSpawn)).toEqual(["dock at 8, 0\u{2013}1, 8 (default)"]);
    expect(countPlaced(TREE)).toBe(2);
    expect(countPlaced([{ id: "0", name: "Empty" }])).toBe(0);
  });
});

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

const sent = (type: string) =>
  FakeChannel.posted.filter((e) => e.msg.type === type).map((e) => e.msg);

function deliver(msg: Record<string, unknown>) {
  act(() => {
    for (const ch of FakeChannel.instances) ch.onmessage?.({ data: { to: "page", msg } });
  });
}

const SCENE_META = "inspector::SceneMetadata-v3";
const META = { name: "Beach House", description: "Sand and sun", spawnPoints: [] };

describe("scene card on the live bus with the root active", () => {
  beforeEach(() => {
    FakeChannel.instances = [];
    FakeChannel.posted = [];
    vi.stubGlobal("BroadcastChannel", FakeChannel);
    window.localStorage.setItem("eui-camera-hint-dismissed", "1");
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    window.localStorage.clear();
  });

  it("renames the scene through the metadata component and gives Delete and Duplicate root-specific hints", () => {
    render(<DeWorkspace title="Beach House" viewportSrc="https://catalyst.example.com/_play/?x=1" />);
    deliver({
      type: "scene-ready",
      bridge: 8,
      scene: null,
      frozen: true,
      tool: "translate",
      orientGlobal: false,
      pivotEach: false,
      selected: ["0"],
      active: "0",
    });
    deliver({ type: "selection", selected: ["0"], active: "0", components: { "0": { [SCENE_META]: META } } });

    const card = within(screen.getByRole("region", { name: "Scene properties" }));
    expect(card.getByText("Sand and sun")).toBeTruthy();
    const input = screen.getByRole("textbox", { name: "Scene name" }) as HTMLInputElement;
    expect(input.value).toBe("Beach House");
    input.focus();
    fireEvent.change(input, { target: { value: "  Pier House " } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(sent("set-component")).toEqual([
      {
        type: "set-component",
        entity: "0",
        name: SCENE_META,
        json: JSON.stringify({ ...META, name: "Pier House" }),
      },
    ]);
    const renamed = screen.getByRole("textbox", { name: "Scene name" }) as HTMLInputElement;
    expect(renamed.value).toBe("Pier House");
    fireEvent.blur(renamed);
    expect(sent("set-component")).toHaveLength(1);

    expect((screen.getByLabelText("Delete") as HTMLButtonElement).title).toMatch(/can\u2019t be deleted/);
    expect((screen.getByLabelText("Duplicate") as HTMLButtonElement).title).toMatch(
      /can\u2019t be duplicated/,
    );
  });

  it("shows the scene name as plain text when nothing can write the metadata", () => {
    render(
      <DeWorkspace title="Beach House" tree={TREE} inspector={{ id: "0", name: "Beach House" }} />,
    );
    expect(screen.queryByRole("textbox", { name: "Scene name" })).toBeNull();
    expect(screen.getByTitle("Scene name").textContent).toBe("Beach House");
  });
});
