import { afterEach, beforeEach, expect, test, vi } from "vitest";

const rendererInstances: { loop: (() => void) | null }[] = [];
const controlsInstances: InstanceType<
  typeof import("three/examples/jsm/controls/OrbitControls.js").OrbitControls
>[] = [];

vi.mock("three", async (importOriginal) => {
  const actual = await importOriginal<typeof import("three")>();
  class FakeWebGLRenderer {
    domElement = document.createElement("canvas");
    outputColorSpace = actual.SRGBColorSpace;
    loop: (() => void) | null = null;
    constructor() {
      rendererInstances.push(this);
    }
    setPixelRatio() {}
    setClearColor() {}
    setSize() {}
    setAnimationLoop(cb: (() => void) | null) {
      this.loop = cb;
    }
    render() {}
    dispose() {}
    forceContextLoss() {}
  }
  return { ...actual, WebGLRenderer: FakeWebGLRenderer };
});

vi.mock("three/examples/jsm/controls/OrbitControls.js", async (importOriginal) => {
  const actual = await importOriginal<
    typeof import("three/examples/jsm/controls/OrbitControls.js")
  >();
  class SpyOrbitControls extends actual.OrbitControls {
    constructor(...args: ConstructorParameters<typeof actual.OrbitControls>) {
      super(...args);
      controlsInstances.push(this);
    }
  }
  return { ...actual, OrbitControls: SpyOrbitControls };
});

const { createAvatarScene } = await import("./avatar");

function mockReducedMotion(matches: boolean) {
  window.matchMedia = vi.fn().mockImplementation((query: string) => ({
    matches,
    media: query,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })) as unknown as typeof window.matchMedia;
}

function mount() {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const scene = createAvatarScene(container, { controls: true });
  return {
    scene,
    renderer: rendererInstances.at(-1)!,
    controls: controlsInstances.at(-1)!,
    teardown: () => {
      scene.dispose();
      container.remove();
    },
  };
}

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

beforeEach(() => {
  rendererInstances.length = 0;
  controlsInstances.length = 0;
  mockReducedMotion(true);
});

afterEach(() => {
  vi.restoreAllMocks();
});

test("reduced motion: a drag opens the render loop, damping-tail changes keep it open past a settle window, it closes once idle, and it never opens while paused offscreen", async () => {
  const live = mount();
  expect(live.renderer.loop).toBeNull();

  live.controls.dispatchEvent({ type: "start" });
  expect(live.renderer.loop).toBeTypeOf("function");
  live.controls.dispatchEvent({ type: "end" });
  await wait(250);
  live.controls.dispatchEvent({ type: "change" });
  await wait(250);
  expect(live.renderer.loop).toBeTypeOf("function");
  await wait(500);
  expect(live.renderer.loop).toBeNull();
  live.teardown();

  const paused = mount();
  paused.scene.setActive(false);
  paused.controls.dispatchEvent({ type: "start" });
  expect(paused.renderer.loop).toBeNull();
  paused.teardown();
});

test("normal motion: the continuous loop already runs, so OrbitControls activity never swaps it for a demand loop", () => {
  mockReducedMotion(false);
  const { renderer, controls, teardown } = mount();
  expect(renderer.loop).toBeTypeOf("function");
  const continuousLoop = renderer.loop;
  controls.dispatchEvent({ type: "start" });
  expect(renderer.loop).toBe(continuousLoop);
  teardown();
});
