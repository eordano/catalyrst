import { afterEach, beforeEach, expect, test, vi } from "vitest";
import * as THREE from "three";

type RenderSample = { visible: boolean; hipsY: number | null };
type FakeRenderer = { loop: (() => void) | null; lastScene: THREE.Object3D | null; camera: THREE.PerspectiveCamera | null };
const rendererInstances: FakeRenderer[] = [];
const renders: RenderSample[] = [];
const loads: string[] = [];
const deferred = new Map<string, () => void>();
let autoResolve = true;
let tinyBindPose = false;

const BODY = "urn:decentraland:off-chain:base-avatars:BaseMale";
const FEMALE = "urn:decentraland:off-chain:base-avatars:BaseFemale";
const HAT1 = "urn:decentraland:matic:collections-v2:0xhat:1";
const HAT2 = "urn:decentraland:matic:collections-v2:0xhat:2";
const HAT3 = "urn:decentraland:matic:collections-v2:0xhat:3";
const HAT4 = "urn:decentraland:matic:collections-v2:0xhat:4";
const HAT5 = "urn:decentraland:matic:collections-v2:0xhat:5";
const HAT6 = "urn:decentraland:matic:collections-v2:0xhat:6";

const contentUrl = (pointer: string) =>
  `${window.location.origin}/content/contents/h_${pointer.toLowerCase()}`;
const partName = (pointer: string) => `part:${pointer.toLowerCase()}`;

vi.mock("three", async (importOriginal) => {
  const actual = await importOriginal<typeof import("three")>();
  class FakeWebGLRenderer implements FakeRenderer {
    domElement = document.createElement("canvas");
    outputColorSpace = actual.SRGBColorSpace;
    loop: (() => void) | null = null;
    lastScene: THREE.Object3D | null = null;
    camera: THREE.PerspectiveCamera | null = null;
    constructor() {
      rendererInstances.push(this);
    }
    setPixelRatio() {}
    setClearColor() {}
    setSize() {}
    setAnimationLoop(cb: (() => void) | null) {
      this.loop = cb;
    }
    render(scene: THREE.Object3D, camera: THREE.PerspectiveCamera) {
      this.lastScene = scene;
      this.camera = camera;
      const group = scene.getObjectByName("avatar");
      const hips = scene.getObjectByName("Avatar_Hips");
      renders.push({
        visible: Boolean(scene.visible && group?.visible),
        hipsY: hips ? hips.position.y : null,
      });
    }
    dispose() {}
    forceContextLoss() {}
  }
  return { ...actual, WebGLRenderer: FakeWebGLRenderer };
});

vi.mock("three/examples/jsm/loaders/GLTFLoader.js", () => {
  const idleClip = () =>
    new THREE.AnimationClip("idle", 1, [
      new THREE.VectorKeyframeTrack("Avatar_Hips.position", [0, 1], [0, 1, 0, 0, 1, 0]),
      new THREE.VectorKeyframeTrack("Avatar_Hips.scale", [0, 1], [1, 1, 1, 1, 1, 1]),
    ]);
  const strayClip = () =>
    new THREE.AnimationClip("wave", 1, [
      new THREE.VectorKeyframeTrack("Somebody_Else.position", [0, 1], [0, 5, 0, 0, 5, 0]),
    ]);
  const fakePart = (pointer: string) => {
    const g = new THREE.Group();
    g.name = `part:${pointer}`;
    const hips = new THREE.Object3D();
    hips.name = "Avatar_Hips";
    if (tinyBindPose) hips.scale.setScalar(0.01);
    g.add(hips);
    const mesh = new THREE.Mesh(
      new THREE.BoxGeometry(0.5, 1.8, 0.3),
      new THREE.MeshStandardMaterial({ name: "skin" }),
    );
    mesh.name = pointer === BODY.toLowerCase() ? "ubody_basemesh" : `m_${pointer}`;
    hips.add(mesh);
    return g;
  };
  class FakeGLTFLoader {
    loadAsync(url: string) {
      loads.push(url);
      const gltf = url.includes("idle.glb")
        ? { scene: new THREE.Group(), animations: [idleClip()] }
        : url.includes("wave.glb")
          ? { scene: new THREE.Group(), animations: [strayClip()] }
          : { scene: fakePart(url.split("h_").pop() ?? ""), animations: [] };
      if (autoResolve) return Promise.resolve(gltf);
      return new Promise((resolve) => deferred.set(url, () => resolve(gltf)));
    }
  }
  return { GLTFLoader: FakeGLTFLoader };
});

const { createAvatarScene, OUTFIT_SWAP_DEBOUNCE_MS } = await import("./avatar");

function entityFor(pointer: string) {
  const p = pointer.toLowerCase();
  return {
    pointers: [p],
    content: [{ file: "model.glb", hash: `h_${p}` }],
    metadata: {
      data: {
        category: [BODY, FEMALE].some(body => body.toLowerCase() === p) ? "body_shape" : "hat",
        representations: [{ bodyShapes: [BODY], mainFile: "model.glb" }],
      },
    },
  };
}

const flush = (ms = 5) => new Promise((r) => setTimeout(r, ms));
const resolveLoad = (url: string) => {
  const fn = deferred.get(url);
  if (!fn) throw new Error(`no pending load for ${url}`);
  deferred.delete(url);
  fn();
};
const neverShowedBindPose = () => renders.every((r) => !r.visible || r.hipsY === 1);

beforeEach(() => {
  rendererInstances.length = 0;
  renders.length = 0;
  loads.length = 0;
  deferred.clear();
  autoResolve = true;
  tinyBindPose = false;
  window.matchMedia = vi.fn().mockImplementation((query: string) => ({
    matches: false,
    media: query,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })) as unknown as typeof window.matchMedia;
  globalThis.fetch = vi.fn(async (_url: string | URL | Request, init?: RequestInit) => {
    const body = JSON.parse(String(init?.body ?? "{}")) as { pointers?: string[] };
    return { ok: true, json: async () => (body.pointers ?? []).map(entityFor) } as Response;
  }) as unknown as typeof fetch;
});

afterEach(() => {
  vi.restoreAllMocks();
});

function mount(urns: string[], emote = "idle") {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const onStatus = vi.fn();
  const scene = createAvatarScene(container, { body: BODY, urns, emote, onStatus });
  const renderer = rendererInstances.at(-1)!;
  const mountedParts = () => {
    renderer.loop!();
    const group = renderer.lastScene?.getObjectByName("avatar");
    return (group?.children ?? []).map((c) => c.name);
  };
  const partObject = (pointer: string) =>
    renderer.lastScene?.getObjectByName(partName(pointer)) ?? null;
  return { container, scene, onStatus, renderer, mountedParts, partObject };
}

test("initial load: no frame shows the avatar before the idle clip is bound (no bind pose)", async () => {
  autoResolve = false;
  const { container, scene, onStatus, renderer } = mount([HAT1]);
  await flush();
  await flush();
  expect(loads).toContain(contentUrl(BODY));
  expect(loads).toContain(contentUrl(HAT1));

  renderer.loop!();
  expect(renders.at(-1)).toEqual({ visible: false, hipsY: null });

  resolveLoad(contentUrl(BODY));
  resolveLoad(contentUrl(HAT1));
  await flush();
  await flush();
  const idleUrl = loads.find((u) => u.includes("idle.glb"));
  expect(idleUrl).toBeTruthy();

  renderer.loop!();
  expect(renders.at(-1)).toEqual({ visible: false, hipsY: 0 });
  expect(onStatus).not.toHaveBeenCalledWith("ready");

  resolveLoad(idleUrl!);
  await flush();
  await flush();
  renderer.loop!();
  expect(renders.at(-1)).toEqual({ visible: true, hipsY: 1 });
  expect(onStatus).toHaveBeenLastCalledWith("ready");
  expect(neverShowedBindPose()).toBe(true);

  scene.dispose();
  container.remove();
});

test("swaps keep the loaded body, load only the changed wearable, reuse the cache on the way back, and a fast sweep loads only the final target with superseded loads never mounting", async () => {
  const { container, scene, onStatus, mountedParts, partObject } = mount([HAT1]);
  await vi.waitFor(() => expect(onStatus).toHaveBeenLastCalledWith("ready"));
  expect(mountedParts()).toEqual([partName(BODY), partName(HAT1)]);
  const body = partObject(BODY);
  const hat1 = partObject(HAT1);

  loads.length = 0;
  await scene.setOutfit({ body: BODY, urns: [HAT2] });
  expect(loads).toEqual([contentUrl(HAT2)]);
  expect(mountedParts()).toEqual([partName(BODY), partName(HAT2)]);
  expect(partObject(BODY)).toBe(body);
  expect(partObject(HAT2)!.getObjectByName("Avatar_Hips")!.position.y).toBe(1);
  expect(onStatus).toHaveBeenLastCalledWith("ready");

  loads.length = 0;
  await scene.setOutfit({ body: BODY, urns: [HAT1] });
  expect(loads).toEqual([]);
  expect(mountedParts()).toEqual([partName(BODY), partName(HAT1)]);
  expect(partObject(HAT1)).toBe(hat1);
  expect(onStatus.mock.calls.filter(([s]) => s === "loading")).toHaveLength(1);

  loads.length = 0;
  autoResolve = false;
  void scene.setOutfit({ body: BODY, urns: [HAT2] });
  void scene.setOutfit({ body: BODY, urns: [HAT3] });
  const p4 = scene.setOutfit({ body: BODY, urns: [HAT4] });
  await flush(OUTFIT_SWAP_DEBOUNCE_MS + 30);
  expect(loads).toEqual([contentUrl(HAT4)]);

  const p5 = scene.setOutfit({ body: BODY, urns: [HAT5] });
  await flush(OUTFIT_SWAP_DEBOUNCE_MS + 30);
  expect(loads).toEqual([contentUrl(HAT4), contentUrl(HAT5)]);

  resolveLoad(contentUrl(HAT4));
  await p4;
  await flush();
  expect(mountedParts()).toEqual([partName(BODY), partName(HAT1)]);

  resolveLoad(contentUrl(HAT5));
  await p5;
  expect(mountedParts()).toEqual([partName(BODY), partName(HAT5)]);
  expect(neverShowedBindPose()).toBe(true);

  scene.dispose();
  container.remove();
});

test("an initial-load failure is recovered by the next outfit swap on the same scene", async () => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  const fetchOk = globalThis.fetch;
  globalThis.fetch = vi.fn(
    async () => ({ ok: false, status: 503, json: async () => [] }) as unknown as Response,
  ) as unknown as typeof fetch;
  const { container, scene, onStatus, renderer, mountedParts } = mount([HAT6]);
  await vi.waitFor(() => expect(onStatus).toHaveBeenLastCalledWith("error"));
  renderer.loop!();
  expect(renders.at(-1)).toEqual({ visible: false, hipsY: null });

  globalThis.fetch = fetchOk;
  loads.length = 0;
  await scene.setOutfit({ body: BODY, urns: [HAT6] });
  expect(onStatus.mock.calls.map(([s]) => s)).toEqual(["loading", "error", "loading", "ready"]);
  expect(loads).toContain(contentUrl(BODY));
  expect(loads).toContain(contentUrl(HAT6));
  expect(mountedParts()).toEqual([partName(BODY), partName(HAT6)]);
  renderer.loop!();
  expect(renders.at(-1)).toEqual({ visible: true, hipsY: 1 });
  expect(neverShowedBindPose()).toBe(true);

  scene.dispose();
  container.remove();
});

test("frames the posed avatar rather than its miniature bind pose", async () => {
  tinyBindPose = true;
  const { container, scene, onStatus, renderer } = mount([]);
  await vi.waitFor(() => expect(onStatus).toHaveBeenLastCalledWith("ready"));
  renderer.loop!();
  expect(renderer.camera!.far).toBeGreaterThan(50);
  expect(renderer.camera!.position.length()).toBeGreaterThan(2);
  expect(neverShowedBindPose()).toBe(true);
  scene.dispose();
  container.remove();
});

test("a populated outfit restores visibility after an empty outfit hid the scene", async () => {
  const { container, scene, onStatus, renderer } = mount([HAT1]);
  await vi.waitFor(() => expect(onStatus).toHaveBeenLastCalledWith("ready"));
  const fetchOk = globalThis.fetch;
  globalThis.fetch = vi.fn(async () => ({ ok: true, json: async () => [] }) as unknown as Response);
  await scene.setOutfit({ body: "urn:missing-body", urns: [] });
  expect(onStatus).toHaveBeenLastCalledWith("empty");
  renderer.loop!();
  expect(renders.at(-1)?.visible).toBe(false);
  globalThis.fetch = fetchOk;
  await scene.setOutfit({ body: BODY, urns: [HAT1] });
  expect(onStatus).toHaveBeenLastCalledWith("ready");
  renderer.loop!();
  expect(renders.at(-1)?.visible).toBe(true);
  scene.dispose();
  container.remove();
});

test("an emote whose tracks bind to nothing on this avatar falls back to the idle clip", async () => {
  const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
  const { container, scene, onStatus, renderer } = mount([HAT1], "wave");
  await vi.waitFor(() => expect(onStatus).toHaveBeenLastCalledWith("ready"));
  const wave = loads.find((u) => u.includes("wave.glb"));
  const idle = loads.find((u) => u.includes("idle.glb"));
  expect(wave).toBeTruthy();
  expect(idle).toBeTruthy();
  expect(loads.indexOf(idle!)).toBeGreaterThan(loads.indexOf(wave!));
  expect(warn).toHaveBeenCalled();
  renderer.loop!();
  expect(renders.at(-1)).toEqual({ visible: true, hipsY: 1 });
  expect(neverShowedBindPose()).toBe(true);
  scene.dispose();
  container.remove();
});

test("a setOutfit superseded inside the debounce window settles at once instead of hanging, and dispose settles a pending one", async () => {
  const { container, scene, onStatus } = mount([HAT1]);
  await vi.waitFor(() => expect(onStatus).toHaveBeenLastCalledWith("ready"));
  const outcome = (p: Promise<void>) =>
    Promise.race([p.then(() => "settled"), flush(OUTFIT_SWAP_DEBOUNCE_MS / 2).then(() => "pending")]);

  const superseded = scene.setOutfit({ body: BODY, urns: [HAT2] });
  const latest = scene.setOutfit({ body: BODY, urns: [HAT3] });
  expect(await outcome(superseded)).toBe("settled");
  expect(await outcome(latest)).toBe("pending");
  await latest;
  expect(loads.filter((u) => u.includes("h_"))).not.toContain(contentUrl(HAT2));
  expect(loads).toContain(contentUrl(HAT3));

  const pending = scene.setOutfit({ body: BODY, urns: [HAT4] });
  scene.dispose();
  expect(await outcome(pending)).toBe("settled");
  expect(loads).not.toContain(contentUrl(HAT4));
  container.remove();
});

test("legacy body shapes in the wearable list never mount an overlapping body", async () => {
  const { container, scene, mountedParts } = mount([FEMALE, HAT1]);
  await flush(30);
  expect(mountedParts()).toEqual(expect.arrayContaining([partName(BODY), partName(HAT1)]));
  expect(mountedParts()).not.toContain(partName(FEMALE));
  expect(loads).not.toContain(contentUrl(FEMALE));
  await scene.setOutfit({ body: FEMALE, urns: [BODY, HAT1] });
  expect(mountedParts()).toEqual(expect.arrayContaining([partName(FEMALE), partName(HAT1)]));
  expect(mountedParts()).not.toContain(partName(BODY));
  scene.dispose();
  container.remove();
});
