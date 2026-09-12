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

beforeEach(() => {
  rendererInstances.length = 0;
  controlsInstances.length = 0;
  mockReducedMotion(true);
});

afterEach(() => {
  vi.restoreAllMocks();
});

test("reduced motion: a drag ('start') opens the render loop; it closes again after release settles", async () => {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const scene = createAvatarScene(container, { controls: true });

  const renderer = rendererInstances.at(-1)!;
  const controls = controlsInstances.at(-1)!;

  expect(renderer.loop).toBeNull();

  controls.dispatchEvent({ type: "start" });
  expect(renderer.loop).toBeTypeOf("function");

  controls.dispatchEvent({ type: "end" });
  expect(renderer.loop).toBeTypeOf("function");

  await new Promise((r) => setTimeout(r, 700));
  expect(renderer.loop).toBeNull();

  scene.dispose();
  container.remove();
});

test("reduced motion: repeated 'change' events (damping tail) keep the loop open past a single settle window", async () => {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const scene = createAvatarScene(container, { controls: true });

  const renderer = rendererInstances.at(-1)!;
  const controls = controlsInstances.at(-1)!;

  controls.dispatchEvent({ type: "start" });
  controls.dispatchEvent({ type: "end" });
  await new Promise((r) => setTimeout(r, 250));
  controls.dispatchEvent({ type: "change" });
  await new Promise((r) => setTimeout(r, 250));
  expect(renderer.loop).toBeTypeOf("function");

  await new Promise((r) => setTimeout(r, 500));
  expect(renderer.loop).toBeNull();

  scene.dispose();
  container.remove();
});

test("reduced motion + setActive(false): the demand loop does not run while paused offscreen", async () => {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const scene = createAvatarScene(container, { controls: true });

  const renderer = rendererInstances.at(-1)!;
  const controls = controlsInstances.at(-1)!;

  scene.setActive(false);
  controls.dispatchEvent({ type: "start" });
  expect(renderer.loop).toBeNull();

  scene.dispose();
  container.remove();
});

test("normal motion: OrbitControls activity never opens a demand loop (the continuous loop already runs)", async () => {
  mockReducedMotion(false);
  const container = document.createElement("div");
  document.body.appendChild(container);
  const scene = createAvatarScene(container, { controls: true });

  const renderer = rendererInstances.at(-1)!;
  expect(renderer.loop).toBeTypeOf("function");
  const continuousLoop = renderer.loop;

  const controls = controlsInstances.at(-1)!;
  controls.dispatchEvent({ type: "start" });
  expect(renderer.loop).toBe(continuousLoop);

  scene.dispose();
  container.remove();
});
