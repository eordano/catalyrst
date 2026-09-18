import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, render, cleanup, waitFor } from "@testing-library/react";
import WearablePreview, { PREVIEW_LOAD_TIMEOUT_MS } from "./WearablePreview";
import type { AvatarSceneOptions } from "./avatar";

type FakeScene = {
  resize: ReturnType<typeof vi.fn>;
  dispose: ReturnType<typeof vi.fn>;
  setActive: ReturnType<typeof vi.fn>;
  setEmote: ReturnType<typeof vi.fn>;
  setCamera: ReturnType<typeof vi.fn>;
  setOutfit: ReturnType<typeof vi.fn>;
  report: (status: "loading" | "ready" | "error") => void;
  canvas: HTMLCanvasElement;
};
const scenes: FakeScene[] = [];
const createAvatarScene = vi.fn((node: HTMLElement, opts: AvatarSceneOptions): FakeScene => {
  const canvas = document.createElement("canvas");
  node.append(canvas);
  const scene: FakeScene = {
    resize: vi.fn(),
    dispose: vi.fn(() => canvas.remove()),
    setActive: vi.fn(),
    setEmote: vi.fn(async () => {}),
    setCamera: vi.fn(),
    setOutfit: vi.fn(async () => {}),
    report: (status) => opts.onStatus?.(status),
    canvas,
  };
  scenes.push(scene);
  return scene;
});

vi.mock("./avatar", () => ({ createAvatarScene: (node: HTMLElement, opts: AvatarSceneOptions) => createAvatarScene(node, opts) }));

type IOCallback = (entries: { isIntersecting: boolean }[]) => void;
class FakeIntersectionObserver {
  static instances: FakeIntersectionObserver[] = [];
  cb: IOCallback;
  constructor(cb: IOCallback) {
    this.cb = cb;
    FakeIntersectionObserver.instances.push(this);
  }
  observe() {}
  disconnect() {}
  fire(isIntersecting: boolean) {
    this.cb([{ isIntersecting }]);
  }
}

const flush = () => new Promise((r) => setTimeout(r, 0));
const win = window as unknown as { IntersectionObserver?: unknown };

beforeEach(() => {
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
  createAvatarScene.mockClear();
  scenes.length = 0;
  FakeIntersectionObserver.instances.length = 0;
  win.IntersectionObserver = FakeIntersectionObserver;
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

test("a stalled load fails, releases its canvas, ignores late completion, and a retry can load", async () => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  const onStatus = vi.fn();
  const view = render(<WearablePreview key="first" onStatus={onStatus} />);
  await waitFor(() => expect(scenes).toHaveLength(1));
  const first = scenes[0]!;
  vi.useFakeTimers();
  view.rerender(<WearablePreview key="stalled" onStatus={onStatus} />);
  await act(async () => { await vi.advanceTimersByTimeAsync(0); });
  const stalled = scenes.at(-1)!;
  expect(stalled).not.toBe(first);
  await act(async () => { await vi.advanceTimersByTimeAsync(PREVIEW_LOAD_TIMEOUT_MS); });
  expect(onStatus).toHaveBeenLastCalledWith("error");
  expect(stalled.dispose).toHaveBeenCalledTimes(1);
  expect(view.container.querySelector("canvas")).toBeNull();
  act(() => stalled.report("ready"));
  expect(onStatus).toHaveBeenLastCalledWith("error");
  view.rerender(<WearablePreview key="retry" onStatus={onStatus} />);
  await act(async () => { await vi.advanceTimersByTimeAsync(0); });
  act(() => scenes.at(-1)!.report("ready"));
  await act(async () => { await vi.advanceTimersByTimeAsync(PREVIEW_LOAD_TIMEOUT_MS); });
  expect(onStatus).toHaveBeenLastCalledWith("ready");
  expect(view.container.querySelector("canvas")).not.toBeNull();
});

test("context loss after readiness reports failure through the current callback and releases the blank canvas", async () => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  const oldStatus = vi.fn();
  const onStatus = vi.fn();
  const view = render(<WearablePreview onStatus={oldStatus} />);
  await waitFor(() => expect(scenes).toHaveLength(1));
  const scene = scenes[0]!;
  act(() => scene.report("ready"));
  view.rerender(<WearablePreview onStatus={onStatus} />);
  act(() => scene.canvas.dispatchEvent(new Event("webglcontextlost", { cancelable: true })));
  expect(onStatus).toHaveBeenLastCalledWith("error");
  expect(oldStatus).toHaveBeenLastCalledWith("ready");
  expect(scene.dispose).toHaveBeenCalledTimes(1);
  expect(view.container.querySelector("[data-status='error']")).not.toBeNull();
  view.unmount();
  expect(scene.dispose).toHaveBeenCalledTimes(1);
});

test("a stalled outfit reload after readiness still times out", async () => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  const onStatus = vi.fn();
  const view = render(<WearablePreview onStatus={onStatus} body="male" />);
  await waitFor(() => expect(scenes).toHaveLength(1));
  const scene = scenes[0]!;
  act(() => scene.report("ready"));
  vi.useFakeTimers();
  scene.setOutfit.mockImplementation(async () => scene.report("loading"));
  view.rerender(<WearablePreview onStatus={onStatus} body="female" />);
  expect(onStatus).toHaveBeenLastCalledWith("loading");
  await act(async () => { await vi.advanceTimersByTimeAsync(PREVIEW_LOAD_TIMEOUT_MS); });
  expect(onStatus).toHaveBeenLastCalledWith("error");
  expect(scene.dispose).toHaveBeenCalledTimes(1);
  expect(view.container.querySelector("canvas")).toBeNull();
  act(() => scene.report("ready"));
  expect(onStatus).toHaveBeenLastCalledWith("error");
});

test("boots on mount when pauseOffscreen is off, and when pauseOffscreen is on but the engine has no IntersectionObserver", async () => {
  render(<WearablePreview />);
  await waitFor(() => expect(createAvatarScene).toHaveBeenCalledTimes(1));
  cleanup();

  const saved = win.IntersectionObserver;
  // @ts-expect-error -- simulating an engine with no IntersectionObserver support.
  delete window.IntersectionObserver;
  try {
    render(<WearablePreview pauseOffscreen />);
    await waitFor(() => expect(createAvatarScene).toHaveBeenCalledTimes(2));
  } finally {
    win.IntersectionObserver = saved;
  }
});

test("pauseOffscreen with an IntersectionObserver waits for the tile to be reported visible and ignores non-intersecting reports", async () => {
  render(<WearablePreview pauseOffscreen />);
  await flush();
  expect(createAvatarScene).not.toHaveBeenCalled();

  const io = FakeIntersectionObserver.instances.at(-1)!;
  io.fire(false);
  await flush();
  expect(createAvatarScene).not.toHaveBeenCalled();

  io.fire(true);
  await waitFor(() => expect(createAvatarScene).toHaveBeenCalledTimes(1));
});

test("an outfit change swaps on the live scene once per change, while a model change rebuilds it", async () => {
  const hat = { bodyShape: "urn:body", wearables: ["urn:hat:1"] };
  const { rerender } = render(<WearablePreview outfit={hat} emote="idle" />);
  await waitFor(() => expect(createAvatarScene).toHaveBeenCalledTimes(1));
  const scene = scenes[0]!;
  expect(scene.setOutfit).not.toHaveBeenCalled();

  const shirt = { bodyShape: "urn:body", wearables: ["urn:hat:1", "urn:shirt:2"] };
  rerender(<WearablePreview outfit={shirt} emote="idle" />);
  await waitFor(() => expect(scene.setOutfit).toHaveBeenCalledTimes(1));
  expect(scene.setOutfit).toHaveBeenLastCalledWith(expect.objectContaining({ outfit: shirt }));
  expect(createAvatarScene).toHaveBeenCalledTimes(1);
  expect(scene.dispose).not.toHaveBeenCalled();

  rerender(<WearablePreview outfit={shirt} emote="idle" />);
  await flush();
  expect(scene.setOutfit).toHaveBeenCalledTimes(1);

  rerender(<WearablePreview outfit={shirt} emote="idle" model="/b.glb" />);
  await waitFor(() => expect(createAvatarScene).toHaveBeenCalledTimes(2));
  expect(scene.dispose).toHaveBeenCalledTimes(1);
});
