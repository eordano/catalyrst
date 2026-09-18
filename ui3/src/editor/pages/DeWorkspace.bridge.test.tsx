import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import DeWorkspace from "./DeWorkspace";
import * as projectCache from "../project-cache";
import { RESTORE_COMPOSITE_TIMEOUT_MS, RPC_TIMEOUT_MS } from "../editor-config";

type Envelope = { to: string; msg: Record<string, unknown> };
class Channel {
  static instances: Channel[] = [];
  static sent: Envelope[] = [];
  onmessage: ((event: { data: Envelope }) => void) | null = null;
  closed = false;
  constructor() { Channel.instances.push(this); }
  postMessage(value: Envelope) { Channel.sent.push(value); }
  close() { this.closed = true; }
}
const scene = { hash: "scene", title: "Project", parcels: [], isPortable: false, isBroken: false, isBlocked: false, isSuper: false, sdkVersion: "7" };
const ready = { type: "scene-ready", scene, frozen: false, tool: "translate", orientGlobal: false, pivotEach: false, selected: [], active: null };
const composite = '{"components":[]}';
const messages = (type: string) => Channel.sent.filter(({ msg }) => msg.type === type).map(({ msg }) => msg);
const restores = () => messages("rpc").filter((msg) => msg.method === "restoreComposite");
async function observeEngineReady() {
  await waitFor(() => expect(screen.getByRole("button", { name: "Run the scene" })).toBeEnabled());
}

function deliver(msg: Record<string, unknown>) {
  if (msg.type === "scene-ready") {
    const frame = document.querySelector<HTMLIFrameElement>('iframe');
    if (frame?.contentWindow) Object.assign(frame.contentWindow, { engine_console_command: async () => "[]" });
  }
  act(() => {
    for (const channel of Channel.instances) if (!channel.closed) channel.onmessage?.({ data: { to: "page", msg } });
  });
}
beforeEach(() => {
  Channel.instances = []; Channel.sent = [];
  vi.stubGlobal("BroadcastChannel", Channel);
  window.localStorage.setItem("eui-camera-hint-dismissed", "1");
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); window.localStorage.clear(); vi.useRealTimers(); });

it("waits for the scene restore acknowledgement, restores again after iframe reload, and keeps the ribbon", async () => {
  render(<DeWorkspace title="Project" viewportSrc="/_play/?editorSession=00000000-0000-4000-8000-000000000001&realm=project" rawComposite={composite} />);
  expect(document.querySelector('[role="tablist"]')).not.toBeNull();
  deliver(ready);
  await waitFor(() => expect(restores()).toHaveLength(1));
  expect(document.querySelector('.eui-boot:not(.is-leaving)')).not.toBeNull();
  deliver(ready);
  await waitFor(() => expect(restores()).toHaveLength(1));
  deliver({ type: "rpc-reply", id: restores()[0]!.id, ok: true });
  await waitFor(() => expect(document.querySelector('.eui-boot:not(.is-leaving)')).toBeNull());
  const previous = Channel.instances[0]!;
  fireEvent.load(screen.getByTitle("Scene viewport"));
  expect(previous.closed).toBe(true);
  deliver(ready);
  await waitFor(() => expect(restores()).toHaveLength(2));
  expect(document.querySelector('.eui-boot:not(.is-leaving)')).not.toBeNull();
  deliver({ type: "rpc-reply", id: restores()[1]!.id, ok: true });
  await waitFor(() => expect(document.querySelector('.eui-boot:not(.is-leaving)')).toBeNull());
});

it("does not suppress hydration when another workspace opens identical saved contents", async () => {
  const first = render(<DeWorkspace viewportSrc="/_play?editorSession=00000000-0000-4000-8000-000000000001&project=a" rawComposite={composite} />);
  deliver(ready);
  await waitFor(() => expect(restores()).toHaveLength(1));
  first.unmount();
  render(<DeWorkspace viewportSrc="/_play?editorSession=00000000-0000-4000-8000-000000000001&project=b" rawComposite={composite} />);
  deliver(ready);
  await waitFor(() => expect(restores()).toHaveLength(2));
});

it("shows a restore failure and retries using a new bridge instead of showing a ready editor", async () => {
  render(<DeWorkspace viewportSrc="/_play?editorSession=00000000-0000-4000-8000-000000000001&project=a" rawComposite={composite} />);
  deliver(ready);
  await waitFor(() => expect(restores()).toHaveLength(1));
  deliver({ type: "rpc-reply", id: restores()[0]!.id, ok: false, error: "offline" });
  expect(await screen.findByRole("alert")).toHaveTextContent("could not be loaded");
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  deliver(ready);
  await waitFor(() => expect(restores()).toHaveLength(2));
});

it("a ready agent without a scene cannot enable live editing", () => {
  render(<DeWorkspace viewportSrc="/_play?editorSession=00000000-0000-4000-8000-000000000001&project=a" rawComposite={composite} />);
  deliver({ ...ready, scene: null });
  expect(restores()).toHaveLength(0);
  expect(document.querySelector('.eui-boot:not(.is-leaving)')).not.toBeNull();
});

it("does not enter Play after failing to preserve the authored scene", async () => {
  render(<DeWorkspace viewportSrc="/_play?editorSession=00000000-0000-4000-8000-000000000001&project=a" />);
  deliver(ready);
  await observeEngineReady();
  fireEvent.keyDown(window, { key: "F5" });
  const snapshot = messages("rpc").find((msg) => msg.method === "exportComposite");
  expect(snapshot).toBeDefined();
  deliver({ type: "rpc-reply", id: snapshot!.id, ok: false, error: "offline" });
  expect(await screen.findByRole("alert")).toHaveTextContent("could not preserve your scene");
  expect(messages("play-state").some((msg) => msg.playing === true)).toBe(false);
});

async function requestPlay() {
  await observeEngineReady();
  fireEvent.keyDown(window, { key: "F5" });
  const snapshot = messages("rpc").find((msg) => msg.method === "exportComposite")!;
  deliver({ type: "rpc-reply", id: snapshot.id, ok: true, result: composite });
  await waitFor(() => expect(messages("rpc").some((msg) => msg.method === "setPlayback")).toBe(true));
  return messages("rpc").filter((msg) => msg.method === "setPlayback").at(-1)!;
}

it("acknowledges Play, Pause, Resume and Stop before advertising their states", async () => {
  render(<DeWorkspace viewportSrc="/_play?editorSession=00000000-0000-4000-8000-000000000001&project=a" />);
  deliver(ready);
  const play = await requestPlay();
  expect(play.args).toEqual([true, false]);
  expect(messages("play-state")).toHaveLength(0);
  deliver({ type: "rpc-reply", id: play.id, ok: true });
  await waitFor(() => expect(messages("play-state").at(-1)).toMatchObject({ playing: true, paused: false }));
  fireEvent.click(screen.getByRole("button", { name: "Scene is running \u2014 pause" }));
  const pause = messages("rpc").filter((msg) => msg.method === "setPlayback").at(-1)!;
  expect(pause.args).toEqual([true, true]);
  expect(messages("play-state").at(-1)).toMatchObject({ paused: false });
  deliver({ type: "rpc-reply", id: pause.id, ok: true });
  await waitFor(() => expect(messages("play-state").at(-1)).toMatchObject({ playing: true, paused: true }));
  fireEvent.keyDown(window, { key: "F5" });
  await waitFor(() => expect(messages("rpc").filter((msg) => msg.method === "setPlayback")).toHaveLength(3));
  const resume = messages("rpc").filter((msg) => msg.method === "setPlayback").at(-1)!;
  expect(resume.args).toEqual([true, false]);
  deliver({ type: "rpc-reply", id: resume.id, ok: true });
  await waitFor(() => expect(messages("play-state").at(-1)).toMatchObject({ paused: false }));
  fireEvent.click(screen.getByRole("button", { name: "Stop the preview and return to editing" }));
  await waitFor(() => expect(messages("rpc").filter((msg) => msg.method === "stopPlayback")).toHaveLength(1));
  expect(messages("play-state").at(-1)).toMatchObject({ playing: true });
  const stop = messages("rpc").find((msg) => msg.method === "stopPlayback")!;
  expect(stop.args).toEqual([composite]);
  deliver({ type: "rpc-reply", id: stop.id, ok: true });
  await waitFor(() => expect(messages("play-state").at(-1)).toMatchObject({ playing: false, paused: false }));
});

it("shows an engine Play rejection without claiming that preview started", async () => {
  render(<DeWorkspace viewportSrc="/_play?editorSession=00000000-0000-4000-8000-000000000001&project=a" />);
  deliver(ready);
  const play = await requestPlay();
  deliver({ type: "rpc-reply", id: play.id, ok: false, error: "Project is disconnected" });
  expect(await screen.findByRole("alert")).toHaveTextContent("Project is disconnected");
  expect(messages("play-state")).toHaveLength(0);
  expect(screen.queryByRole("button", { name: "Stop the preview and return to editing" })).toBeNull();
});

it("keeps the running controls after a rejected Pause", async () => {
  render(<DeWorkspace viewportSrc="/_play?editorSession=00000000-0000-4000-8000-000000000001&project=a" />);
  deliver(ready);
  const play = await requestPlay();
  deliver({ type: "rpc-reply", id: play.id, ok: true });
  fireEvent.click(await screen.findByRole("button", { name: "Scene is running \u2014 pause" }));
  const pause = messages("rpc").filter((msg) => msg.method === "setPlayback").at(-1)!;
  deliver({ type: "rpc-reply", id: pause.id, ok: false, error: "Disconnected" });
  expect(await screen.findByRole("alert")).toHaveTextContent("could not be paused");
  expect(screen.queryByRole("button", { name: "Run the scene" })).toBeNull();
  expect(messages("play-state").at(-1)).toMatchObject({ playing: true, paused: false });
});

it("registers folder content before restoring a saved composite", async () => {
  const preparePreview = vi.fn(async () => ({ "assets/model.glb": "b64-model" }));
  const assets = { preparePreview, list: async () => [], read: async () => ({ content: new ArrayBuffer(0), revision: "" }), write: async () => {}, remove: async () => {} };
  render(<DeWorkspace viewportSrc="/_play?editorSession=00000000-0000-4000-8000-000000000001&project=folder" rawComposite={composite} code={{ project: { id: "folder", list: async () => [], read: async () => "", write: async () => {}, assets } }} />);
  deliver(ready);
  expect(restores()).toHaveLength(0);
  await waitFor(() => expect(messages("rpc").some(msg => msg.method === "registerContent")).toBe(true));
  const register = messages("rpc").find(msg => msg.method === "registerContent")!;
  expect(register.args).toEqual([{ "assets/model.glb": "b64-model" }]);
  expect(restores()).toHaveLength(0);
  deliver({ type: "rpc-reply", id: register.id, ok: true, result: 1 });
  await waitFor(() => expect(restores()).toHaveLength(1));
  expect(document.querySelector('.eui-boot:not(.is-leaving)')).not.toBeNull();
  deliver({ type: "rpc-reply", id: restores()[0]!.id, ok: true });
  await waitFor(() => expect(document.querySelector('.eui-boot:not(.is-leaving)')).toBeNull());
});


it("accepts scene restoration after the ordinary RPC deadline", async () => {
  vi.useFakeTimers();
  render(<DeWorkspace viewportSrc="/_play?editorSession=00000000-0000-4000-8000-000000000001&project=slow" rawComposite={composite} />);
  Object.assign((screen.getByTitle("Scene viewport") as HTMLIFrameElement).contentWindow!, { engine_console_command: async () => "[]" });
  deliver(ready);
  await act(async () => { await vi.advanceTimersByTimeAsync(RPC_TIMEOUT_MS + 1_000); });
  expect(screen.queryByRole("alert")).toBeNull();
  expect(document.querySelector('.eui-boot:not(.is-leaving)')).not.toBeNull();
  await act(async () => { deliver({ type: "rpc-reply", id: restores()[0]!.id, ok: true }); });
  expect(document.querySelector('.eui-boot:not(.is-leaving)')).toBeNull();
});

it("offers retry when scene restoration exceeds its bounded deadline", async () => {
  vi.useFakeTimers();
  render(<DeWorkspace viewportSrc="/_play?editorSession=00000000-0000-4000-8000-000000000001&project=stalled" rawComposite={composite} />);
  deliver(ready);
  await act(async () => { await vi.advanceTimersByTimeAsync(RESTORE_COMPOSITE_TIMEOUT_MS + 1); });
  expect(screen.getByRole("alert")).toHaveTextContent("could not be loaded");
  expect(screen.getByRole("button", { name: "Retry" })).toBeVisible();
});

it("updates the header and saved scene name only after rename acknowledgement", async () => {
  const changed = vi.fn();
  render(<DeWorkspace title="Project" viewportSrc="/_play?editorSession=00000000-0000-4000-8000-000000000001&project=rename" sceneInfo={{ base: "2,-3", parcels: ["2,-3", "3,-3"] }} onSceneNameChange={changed}
    renderHeader={(navigation, scene) => <>{navigation}<span data-testid="header-name">{scene.name}</span><button onClick={() => scene.rename?.("Moonlit Garden")}>Rename header</button></>} />);
  deliver({ ...ready, scene: { ...ready.scene, title: "My New Scene" } });
  await observeEngineReady();
  fireEvent.click(screen.getByRole("button", { name: "Rename header" }));
  const write = messages("rpc").find(message => message.method === "writeComponents")!;
  expect(write.args).toEqual(["0", [{ name: "inspector::SceneMetadata-v3", value: { name: "Moonlit Garden", layout: { base: { x: 2, y: -3 }, parcels: [{ x: 2, y: -3 }, { x: 3, y: -3 }] } } }]]);
  expect(screen.getByTestId("header-name").textContent).toBe("Project");
  expect(changed).not.toHaveBeenCalledWith("Moonlit Garden");
  deliver({ type: "rpc-reply", id: write.id, ok: true });
  await waitFor(() => expect(screen.getByTestId("header-name").textContent).toBe("Moonlit Garden"));
  expect(changed).toHaveBeenLastCalledWith("Moonlit Garden");
});


it("rejects repeated Play while preserving the snapshot and ignores a reply after reconnect", async () => {
  render(<DeWorkspace viewportSrc="/_play?editorSession=00000000-0000-4000-8000-000000000001&project=a" />);
  deliver(ready);
  await observeEngineReady();
  fireEvent.keyDown(window, { key: "F5" });
  fireEvent.keyDown(window, { key: "F5" });
  const snapshots = messages("rpc").filter(msg => msg.method === "exportComposite");
  expect(snapshots).toHaveLength(1);
  fireEvent.load(screen.getByTitle("Scene viewport"));
  deliver({ type: "rpc-reply", id: snapshots[0]!.id, ok: true, result: composite });
  await act(async () => {});
  expect(messages("rpc").filter(msg => msg.method === "setPlayback")).toHaveLength(0);
  deliver(ready);
  await observeEngineReady();
  fireEvent.keyDown(window, { key: "F5" });
  expect(messages("rpc").filter(msg => msg.method === "exportComposite")).toHaveLength(2);
});

it("does not start playback while a page save is active", () => {
  render(<DeWorkspace viewportSrc="/_play?editorSession=00000000-0000-4000-8000-000000000001&project=a" operationsBusy />);
  deliver(ready);
  fireEvent.keyDown(window, { key: "F5" });
  expect(messages("rpc").filter(msg => msg.method === "exportComposite")).toHaveLength(0);
});

it("publishes ordered full snapshots and delivers current state to a new subscriber", async () => {
  const updates = vi.fn();
  const view = render(<DeWorkspace viewportSrc="/_play?editorSession=00000000-0000-4000-8000-000000000001&project=a" onLifecycle={updates} />);
  deliver(ready);
  await waitFor(() => expect(updates.mock.calls.at(-1)?.[0]).toMatchObject({ ready: true, phase: "editing", busy: false }));
  const request = await requestPlay();
  expect(updates.mock.calls.at(-1)?.[0]).toMatchObject({ playing: false, busy: true, request: { command: "play" } });
  deliver({ type: "rpc-reply", id: request.id, ok: true });
  await waitFor(() => expect(updates.mock.calls.at(-1)?.[0]).toMatchObject({ phase: "playing", busy: false, outcome: { error: null } }));
  const sequences = updates.mock.calls.map(([snapshot]) => snapshot.sequence as number);
  expect(sequences.every((value, index) => index === 0 || value > sequences[index - 1]!)).toBe(true);
  const reattached = vi.fn();
  view.rerender(<DeWorkspace viewportSrc="/_play?editorSession=00000000-0000-4000-8000-000000000001&project=a" onLifecycle={reattached} />);
  expect(reattached).toHaveBeenCalledOnce();
  expect(reattached.mock.calls[0]?.[0]).toMatchObject({ ready: true, phase: "playing", playing: true, request: null });
});

it("rotates the transport namespace on Retry before accepting a replacement scene", async () => {
  const updates = vi.fn();
  render(<DeWorkspace viewportSrc="/_play?editorSession=00000000-0000-4000-8000-000000000001" rawComposite={composite} onLifecycle={updates} />);
  deliver(ready);
  await waitFor(() => expect(restores()).toHaveLength(1));
  deliver({ type: "rpc-reply", id: restores()[0]!.id, ok: false, error: "disconnected" });
  await screen.findByRole("alert");
  const before = (screen.getByTitle("Scene viewport") as HTMLIFrameElement).src;
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  const after = (screen.getByTitle("Scene viewport") as HTMLIFrameElement).src;
  expect(new URL(after).searchParams.get("editorSession")).not.toBe(new URL(before).searchParams.get("editorSession"));
  expect(updates.mock.calls.at(-1)?.[0].destination).toBe(after);
});


it("restores the running cache flag after a rejected Stop and preserves its snapshot for retry", async () => {
  const write = vi.spyOn(projectCache, "setProjectPlayState").mockResolvedValue(undefined);
  render(<DeWorkspace viewportSrc="/_play?editorSession=00000000-0000-4000-8000-000000000001&project=a" />);
  deliver(ready);
  await waitFor(() => expect(screen.getByRole("button", { name: "Run the scene" })).not.toBeDisabled());
  const play = await requestPlay();
  deliver({ type: "rpc-reply", id: play.id, ok: true });
  const stopButton = await screen.findByRole("button", { name: "Stop the preview and return to editing" });
  write.mockClear();
  fireEvent.click(stopButton);
  await waitFor(() => expect(messages("rpc").some(msg => msg.method === "stopPlayback")).toBe(true));
  const stop = messages("rpc").find(msg => msg.method === "stopPlayback")!;
  expect(write).toHaveBeenLastCalledWith(false);
  deliver({ type: "rpc-reply", id: stop.id, ok: false, error: "offline" });
  expect(await screen.findByRole("alert")).toHaveTextContent("could not be restored");
  expect(write).toHaveBeenLastCalledWith(true);
  expect(messages("play-state").at(-1)).toMatchObject({ playing: true });
  fireEvent.click(screen.getByRole("button", { name: "Stop the preview and return to editing" }));
  await waitFor(() => expect(messages("rpc").filter(msg => msg.method === "stopPlayback")).toHaveLength(2));
  const retry = messages("rpc").filter(msg => msg.method === "stopPlayback").at(-1)!;
  expect(retry.args).toEqual([composite]);
  deliver({ type: "rpc-reply", id: retry.id, ok: true });
  await waitFor(() => expect(messages("play-state").at(-1)).toMatchObject({ playing: false }));
  expect(write).toHaveBeenLastCalledWith(false);
});
