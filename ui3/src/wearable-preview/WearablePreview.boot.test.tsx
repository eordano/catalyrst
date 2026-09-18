import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { render, cleanup, waitFor } from "@testing-library/react";
import WearablePreview from "./WearablePreview";

type FakeScene = {
  resize: ReturnType<typeof vi.fn>;
  dispose: ReturnType<typeof vi.fn>;
  setActive: ReturnType<typeof vi.fn>;
  setEmote: ReturnType<typeof vi.fn>;
  setCamera: ReturnType<typeof vi.fn>;
  setOutfit: ReturnType<typeof vi.fn>;
};
const scenes: FakeScene[] = [];
const createAvatarScene = vi.fn((): FakeScene => {
  const scene: FakeScene = {
    resize: vi.fn(),
    dispose: vi.fn(),
    setActive: vi.fn(),
    setEmote: vi.fn(async () => {}),
    setCamera: vi.fn(),
    setOutfit: vi.fn(async () => {}),
  };
  scenes.push(scene);
  return scene;
});

vi.mock("./avatar", () => ({ createAvatarScene: () => createAvatarScene() }));

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
  createAvatarScene.mockClear();
  scenes.length = 0;
  FakeIntersectionObserver.instances.length = 0;
  win.IntersectionObserver = FakeIntersectionObserver;
});

afterEach(() => {
  cleanup();
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
  expect(scene.setOutfit).toHaveBeenLastCalledWith({
    profile: undefined,
    urns: undefined,
    body: undefined,
    outfit: shirt,
  });
  expect(createAvatarScene).toHaveBeenCalledTimes(1);
  expect(scene.dispose).not.toHaveBeenCalled();

  rerender(<WearablePreview outfit={shirt} emote="idle" />);
  await flush();
  expect(scene.setOutfit).toHaveBeenCalledTimes(1);

  rerender(<WearablePreview outfit={shirt} emote="idle" model="/b.glb" />);
  await waitFor(() => expect(createAvatarScene).toHaveBeenCalledTimes(2));
  expect(scene.dispose).toHaveBeenCalledTimes(1);
});
