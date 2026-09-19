import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import DeWorkspace from "./DeWorkspace";

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
function deliver(msg: Record<string, unknown>) {
  act(() => {
    for (const channel of Channel.instances) if (!channel.closed) channel.onmessage?.({ data: { to: "page", msg } });
  });
}
beforeEach(() => {
  Channel.instances = []; Channel.sent = [];
  vi.stubGlobal("BroadcastChannel", Channel);
  window.localStorage.setItem("eui-camera-hint-dismissed", "1");
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); window.localStorage.clear(); });

it("waits for the scene restore acknowledgement, restores again after iframe reload, and keeps the ribbon", async () => {
  render(<DeWorkspace title="Project" viewportSrc="/_play/?realm=project" rawComposite={composite} />);
  expect(document.querySelector('[role="tablist"]')).not.toBeNull();
  deliver(ready);
  expect(restores()).toHaveLength(1);
  expect(document.querySelector('.eui-boot:not(.is-leaving)')).not.toBeNull();
  deliver(ready);
  expect(restores()).toHaveLength(1);
  deliver({ type: "rpc-reply", id: restores()[0]!.id, ok: true });
  await waitFor(() => expect(document.querySelector('.eui-boot:not(.is-leaving)')).toBeNull());
  const previous = Channel.instances[0]!;
  fireEvent.load(screen.getByTitle("Scene viewport"));
  expect(previous.closed).toBe(true);
  deliver(ready);
  expect(restores()).toHaveLength(2);
  expect(document.querySelector('.eui-boot:not(.is-leaving)')).not.toBeNull();
  deliver({ type: "rpc-reply", id: restores()[1]!.id, ok: true });
  await waitFor(() => expect(document.querySelector('.eui-boot:not(.is-leaving)')).toBeNull());
});

it("does not suppress hydration when another workspace opens identical saved contents", () => {
  const first = render(<DeWorkspace viewportSrc="/_play?project=a" rawComposite={composite} />);
  deliver(ready);
  first.unmount();
  render(<DeWorkspace viewportSrc="/_play?project=b" rawComposite={composite} />);
  deliver(ready);
  expect(restores()).toHaveLength(2);
});

it("shows a restore failure and retries using a new bridge instead of showing a ready editor", async () => {
  render(<DeWorkspace viewportSrc="/_play?project=a" rawComposite={composite} />);
  deliver(ready);
  deliver({ type: "rpc-reply", id: restores()[0]!.id, ok: false, error: "offline" });
  expect(await screen.findByRole("alert")).toHaveTextContent("could not be loaded");
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  deliver(ready);
  expect(restores()).toHaveLength(2);
});

it("a ready agent without a scene cannot enable live editing", () => {
  render(<DeWorkspace viewportSrc="/_play?project=a" rawComposite={composite} />);
  deliver({ ...ready, scene: null });
  expect(restores()).toHaveLength(0);
  expect(document.querySelector('.eui-boot:not(.is-leaving)')).not.toBeNull();
});

it("does not enter Play after failing to preserve the authored scene", async () => {
  render(<DeWorkspace viewportSrc="/_play?project=a" />);
  deliver(ready);
  fireEvent.keyDown(window, { key: "F5" });
  const snapshot = messages("rpc").find((msg) => msg.method === "exportComposite");
  expect(snapshot).toBeDefined();
  deliver({ type: "rpc-reply", id: snapshot!.id, ok: false, error: "offline" });
  expect(await screen.findByRole("alert")).toHaveTextContent("could not preserve your scene");
  expect(messages("play-state").some((msg) => msg.playing === true)).toBe(false);
});

async function requestPlay() {
  fireEvent.keyDown(window, { key: "F5" });
  const snapshot = messages("rpc").find((msg) => msg.method === "exportComposite")!;
  deliver({ type: "rpc-reply", id: snapshot.id, ok: true, result: composite });
  await waitFor(() => expect(messages("rpc").some((msg) => msg.method === "setPlayback")).toBe(true));
  return messages("rpc").filter((msg) => msg.method === "setPlayback").at(-1)!;
}

it("acknowledges Play, Pause, Resume and Stop before advertising their states", async () => {
  render(<DeWorkspace viewportSrc="/_play?project=a" />);
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
  fireEvent.click(screen.getByRole("button", { name: "Stop preview" }));
  await waitFor(() => expect(messages("rpc").filter((msg) => msg.method === "stopPlayback")).toHaveLength(1));
  expect(messages("play-state").at(-1)).toMatchObject({ playing: true });
  const stop = messages("rpc").find((msg) => msg.method === "stopPlayback")!;
  expect(stop.args).toEqual([composite]);
  deliver({ type: "rpc-reply", id: stop.id, ok: true });
  await waitFor(() => expect(messages("play-state").at(-1)).toMatchObject({ playing: false, paused: false }));
});

it("shows an engine Play rejection without claiming that preview started", async () => {
  render(<DeWorkspace viewportSrc="/_play?project=a" />);
  deliver(ready);
  const play = await requestPlay();
  deliver({ type: "rpc-reply", id: play.id, ok: false, error: "Project is disconnected" });
  expect(await screen.findByRole("alert")).toHaveTextContent("Project is disconnected");
  expect(messages("play-state")).toHaveLength(0);
  expect(screen.queryByRole("button", { name: "Stop preview" })).toBeNull();
});

it("keeps the running controls after a rejected Pause", async () => {
  render(<DeWorkspace viewportSrc="/_play?project=a" />);
  deliver(ready);
  const play = await requestPlay();
  deliver({ type: "rpc-reply", id: play.id, ok: true });
  fireEvent.click(await screen.findByRole("button", { name: "Pause preview" }));
  const pause = messages("rpc").filter((msg) => msg.method === "setPlayback").at(-1)!;
  deliver({ type: "rpc-reply", id: pause.id, ok: false, error: "Disconnected" });
  expect(await screen.findByRole("alert")).toHaveTextContent("could not be paused");
  expect(screen.queryByRole("button", { name: "Resume preview" })).toBeNull();
  expect(messages("play-state").at(-1)).toMatchObject({ playing: true, paused: false });
});

it("registers folder content before restoring a saved composite", async () => {
  const preparePreview = vi.fn(async () => ({ "assets/model.glb": "b64-model" }));
  const assets = { preparePreview, list: async () => [], read: async () => ({ content: new ArrayBuffer(0), revision: "" }), write: async () => {}, remove: async () => {} };
  render(<DeWorkspace viewportSrc="/_play?project=folder" rawComposite={composite} code={{ project: { id: "folder", list: async () => [], read: async () => "", write: async () => {}, assets } }} />);
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
