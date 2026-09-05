import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { render, cleanup, waitFor } from "@testing-library/react";
import WearablePreview from "./WearablePreview";

// Charter item 1 (avatar boot-on-visible): the scene-creating effect is gated
// on `booted`, which starts false only for pauseOffscreen consumers with
// IntersectionObserver support -- every other combination boots on mount
// exactly as before the change. createAvatarScene stands in for "did the
// scene (and so its GLB fetches) actually boot".

const createAvatarScene = vi.fn(() => ({
  resize: vi.fn(),
  dispose: vi.fn(),
  setActive: vi.fn(),
  setEmote: vi.fn(async () => {}),
  setCamera: vi.fn(),
}));

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

beforeEach(() => {
  createAvatarScene.mockClear();
  FakeIntersectionObserver.instances.length = 0;
});

afterEach(() => {
  cleanup();
});

test("booted inits true when pauseOffscreen is falsy \u{2014} boots on mount regardless of IntersectionObserver", async () => {
  (window as unknown as { IntersectionObserver: unknown }).IntersectionObserver =
    FakeIntersectionObserver;
  render(<WearablePreview />);
  await waitFor(() => expect(createAvatarScene).toHaveBeenCalledTimes(1));
});

test("booted inits true when IntersectionObserver is undefined \u{2014} boots on mount even with pauseOffscreen", async () => {
  const saved = (window as unknown as { IntersectionObserver?: unknown }).IntersectionObserver;
  // @ts-expect-error -- simulating an engine with no IntersectionObserver support.
  delete window.IntersectionObserver;
  try {
    render(<WearablePreview pauseOffscreen />);
    await waitFor(() => expect(createAvatarScene).toHaveBeenCalledTimes(1));
  } finally {
    (window as unknown as { IntersectionObserver: unknown }).IntersectionObserver = saved;
  }
});

test("booted inits false when pauseOffscreen + IntersectionObserver: no boot until the tile is reported visible", async () => {
  (window as unknown as { IntersectionObserver: unknown }).IntersectionObserver =
    FakeIntersectionObserver;
  render(<WearablePreview pauseOffscreen />);
  await flush();
  expect(createAvatarScene).not.toHaveBeenCalled();

  const io = FakeIntersectionObserver.instances.at(-1)!;
  io.fire(true);
  await waitFor(() => expect(createAvatarScene).toHaveBeenCalledTimes(1));
});

test("an IO fire with isIntersecting:false never boots the tile", async () => {
  (window as unknown as { IntersectionObserver: unknown }).IntersectionObserver =
    FakeIntersectionObserver;
  render(<WearablePreview pauseOffscreen />);
  await flush();
  const io = FakeIntersectionObserver.instances.at(-1)!;
  io.fire(false);
  await flush();
  expect(createAvatarScene).not.toHaveBeenCalled();
});
