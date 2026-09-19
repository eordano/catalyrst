import * as THREE from "three";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import {
  FACIAL_CATS,
  baseMeshHidden,
  categoryOf,
  computeHiding,
  fetchEntities,
  fileMapFor,
  representationContents,
  representationMainFile,
  resolveOutfit,
} from "./outfit";
import type { Entity, HidingRules, OutfitData, ResolvedOutfit } from "./outfit";

type AvatarColors = { skin: THREE.Color | null; hair: THREE.Color | null; eyes: THREE.Color | null };

const idleUrl = new URL("./emotes/idle.glb", import.meta.url).href;
const waveUrl = new URL("./emotes/wave.glb", import.meta.url).href;
const danceUrl = new URL("./emotes/dance.glb", import.meta.url).href;
const clapUrl = new URL("./emotes/clap.glb", import.meta.url).href;
const dabUrl = new URL("./emotes/dab.glb", import.meta.url).href;

function prefersReducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

export type AvatarStatus = "loading" | "ready" | "empty" | "error";

export interface AvatarSceneOptions {
  base?: string;
  onStatus?: (status: AvatarStatus) => void;
  background?: string;
  fov?: number;
  controls?: boolean;
  pan?: boolean;
  platform?: boolean;
  spin?: boolean;
  spinSpeed?: number;
  targetY?: number;
  zoom?: number;
  yaw?: number;
  pitch?: number;
  model?: string;
  emote?: string;
  emotes?: string[];
  body?: string;
  urns?: string[] | string;
  outfit?: OutfitData | null;
  profile?: string;
}

export type AvatarCameraOptions = Pick<
  AvatarSceneOptions,
  "zoom" | "yaw" | "pitch" | "fov" | "targetY"
>;

export type AvatarOutfitOptions = Pick<AvatarSceneOptions, "profile" | "urns" | "body" | "outfit">;

export interface AvatarScene {
  resize: () => void;
  dispose: () => void;
  setActive: (active: boolean) => void;
  setEmote: (input: string | null | undefined) => Promise<void>;
  setCamera: (next: AvatarCameraOptions) => void;
  setOutfit: (next: AvatarOutfitOptions) => Promise<void>;
}

const EMOTES: Record<string, string> = {
  idle: idleUrl,
  wave: waveUrl,
  dance: danceUrl,
  clap: clapUrl,
  dab: dabUrl,
};

const DEFAULT_BASE = "https://catalyst.example.com";
const PART_CACHE_MAX = 32;
const GLB_TIMEOUT_MS = 20000;
export const OUTFIT_SWAP_DEBOUNCE_MS = 120;

function defaultBase(): string {
  return import.meta.env.SSR ? DEFAULT_BASE : window.location.origin;
}

function isTexture(value: unknown): value is { isTexture: unknown; dispose?: () => void } {
  return (
    typeof value === "object" &&
    value !== null &&
    "isTexture" in value &&
    Boolean((value as { isTexture?: unknown }).isTexture)
  );
}

function disposeObject(root: THREE.Object3D): void {
  root.traverse((o) => {
    if (o.geometry) o.geometry.dispose?.();
    const mats = o.material ? (Array.isArray(o.material) ? o.material : [o.material]) : [];
    for (const m of mats) {
      for (const k in m) {
        const v = m[k];
        if (isTexture(v)) v.dispose?.();
      }
      m.dispose?.();
    }
  });
}

function withTimeout<T>(p: Promise<T>, ms: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const expiry = new Promise<T>((_resolve, reject) => {
    timer = setTimeout(() => reject(new Error("timeout")), ms);
  });
  return Promise.race<T>([p, expiry]).finally(() => {
    if (timer) clearTimeout(timer);
  });
}

const partKey = (urn: string, bodyShape: string): string =>
  `${urn.toLowerCase()}|${bodyShape.toLowerCase()}`;

type FacialFeature = { tex: THREE.Texture; mask: THREE.Texture | null };

interface Part {
  key: string;
  urn: string;
  isBody: boolean;
  obj: THREE.Object3D;
}

export function createAvatarScene(
  container: HTMLElement,
  opts: AvatarSceneOptions = {},
): AvatarScene {
  const base = (opts.base || defaultBase()).replace(/\/$/, "");
  let status: AvatarStatus = "loading";
  const setStatus = (next: AvatarStatus): void => {
    status = next;
    opts.onStatus?.(next);
  };
  let disposed = false;

  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  if (opts.background) renderer.setClearColor(new THREE.Color(opts.background), 1);
  else renderer.setClearColor(0x000000, 0);
  Object.assign(renderer.domElement.style, { width: "100%", height: "100%", display: "block" });
  container.appendChild(renderer.domElement);

  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(opts.fov ?? 34, 1, 0.05, 100);
  camera.position.set(0, 1, 3.5);

  scene.add(new THREE.HemisphereLight(0xffffff, 0xdadae2, 2.0));
  const fill = new THREE.HemisphereLight(0xeef2ff, 0xffffff, 0.7);
  fill.position.set(0, -1, 0);
  scene.add(fill);
  const front = new THREE.DirectionalLight(0xffffff, 0.55);
  front.position.set(0.4, 1.4, 3);
  scene.add(front);

  const interactive = opts.controls !== false;
  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enablePan = !!opts.pan;
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.enabled = interactive;
  controls.enableZoom = interactive;
  controls.enableRotate = interactive;
  controls.minDistance = 0.6;
  controls.maxDistance = 8;

  const reducedMotion = prefersReducedMotion();
  const sway = opts.spin !== false && !reducedMotion;
  const swayAmplitude = THREE.MathUtils.degToRad(60);
  const swaySpeed = (opts.spinSpeed ?? 0.9) * 0.6;
  let swayT = 0;

  const avatarGroup = new THREE.Group();
  avatarGroup.name = "avatar";
  avatarGroup.visible = false;
  scene.add(avatarGroup);

  if (opts.platform) {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 256;
    const ctx = canvas.getContext("2d");
    if (ctx) {
      const g = ctx.createRadialGradient(128, 128, 0, 128, 128, 128);
      g.addColorStop(0, "#ffd76a");
      g.addColorStop(0.45, "#f5b73c");
      g.addColorStop(0.8, "#d8902a");
      g.addColorStop(1, "#a8651b");
      ctx.fillStyle = g;
      ctx.fillRect(0, 0, 256, 256);
    }
    const tex = new THREE.CanvasTexture(canvas);
    tex.colorSpace = THREE.SRGBColorSpace;
    const podium = new THREE.Mesh(new THREE.CylinderGeometry(0.72, 0.78, 0.1, 48), [
      new THREE.MeshBasicMaterial({ color: 0xa8651b }),
      new THREE.MeshBasicMaterial({ map: tex }),
      new THREE.MeshBasicMaterial({ color: 0x7c4a13 }),
    ]);
    podium.position.y = -0.06;
    scene.add(podium);
    controls.maxPolarAngle = Math.PI / 2 - 0.05;
  }

  const partCache = new Map<string, Promise<THREE.Object3D | null>>();
  const featureCache = new Map<string, Promise<FacialFeature | null>>();
  const mounted = new Map<string, Part>();
  const mixers = new Map<string, THREE.AnimationMixer>();
  let currentClip: THREE.AnimationClip | null = null;
  let bodyRoot: THREE.Object3D | null = null;
  let revealed = false;
  let loadGen = 0;
  let swapTimer: ReturnType<typeof setTimeout> | null = null;
  let swapSettle: (() => void) | null = null;
  let lastFrame = performance.now();
  let active = true;

  const stale = (gen: number): boolean => disposed || gen !== loadGen;

  function resize() {
    const w = container.clientWidth || 1;
    const h = container.clientHeight || 1;
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
    renderOnce();
  }
  resize();

  function renderFrame() {
    if (disposed) return;
    const now = performance.now();
    const dt = Math.max(0, (now - lastFrame) / 1000);
    lastFrame = now;
    for (const m of mixers.values()) m.update(reducedMotion ? 0 : dt);
    if (sway) {
      swayT += dt;
      avatarGroup.rotation.y = swayAmplitude * Math.sin(swayT * swaySpeed);
    }
    controls.update();
    renderer.render(scene, camera);
  }
  if (!reducedMotion) renderer.setAnimationLoop(renderFrame);

  let demandLoopActive = false;
  let demandIdleTimer: ReturnType<typeof setTimeout> | null = null;
  const dampingSettleMs = Math.round((2 / controls.dampingFactor) * (1000 / 60));

  function renderOnce(): void {
    if (!reducedMotion || disposed || !active) return;
    renderFrame();
  }

  function stopDemandLoop(): void {
    if (demandIdleTimer) {
      clearTimeout(demandIdleTimer);
      demandIdleTimer = null;
    }
    if (!demandLoopActive) return;
    demandLoopActive = false;
    renderer.setAnimationLoop(null);
  }

  function startDemandLoop(): void {
    if (demandLoopActive || disposed || !active) return;
    demandLoopActive = true;
    lastFrame = performance.now();
    renderer.setAnimationLoop(renderFrame);
  }

  function bumpDemandIdle(): void {
    if (demandIdleTimer) clearTimeout(demandIdleTimer);
    demandIdleTimer = setTimeout(() => {
      demandIdleTimer = null;
      stopDemandLoop();
    }, dampingSettleMs);
  }

  if (reducedMotion) {
    controls.addEventListener("start", () => {
      if (demandIdleTimer) {
        clearTimeout(demandIdleTimer);
        demandIdleTimer = null;
      }
      startDemandLoop();
    });
    controls.addEventListener("change", () => {
      if (demandLoopActive) bumpDemandIdle();
    });
    controls.addEventListener("end", bumpDemandIdle);
  }

  function setActive(next: boolean): void {
    if (disposed || next === active) return;
    active = next;
    lastFrame = performance.now();
    if (reducedMotion) {
      if (next) renderOnce();
      else stopDemandLoop();
    } else {
      renderer.setAnimationLoop(next ? renderFrame : null);
    }
  }

  function frame() {
    avatarGroup.updateWorldMatrix(true, true);
    const box = new THREE.Box3().setFromObject(avatarGroup, true);
    if (box.isEmpty()) return;
    const size = box.getSize(new THREE.Vector3());
    const center = box.getCenter(new THREE.Vector3());
    avatarGroup.position.x -= center.x;
    avatarGroup.position.z -= center.z;
    avatarGroup.position.y -= box.min.y;
    const h = size.y || 1.8;
    const ty = h * (opts.targetY ?? 0.5);
    const dist = (h * 2.15) / Math.max(0.2, opts.zoom ?? 1);
    controls.target.set(0, ty, 0);
    controls.minDistance = dist * 0.35;
    controls.maxDistance = dist * 3.5;
    camera.near = h / 100;
    camera.far = h * 40;
    camera.updateProjectionMatrix();
    const yaw = THREE.MathUtils.degToRad(opts.yaw ?? 0);
    const pitch = THREE.MathUtils.degToRad(opts.pitch ?? 20);
    const horiz = dist * Math.cos(pitch);
    camera.position.set(horiz * Math.sin(yaw), ty + dist * Math.sin(pitch), horiz * Math.cos(yaw));
    controls.update();
    renderOnce();
  }

  function reveal(): void {
    if (revealed) return;
    revealed = true;
    avatarGroup.visible = true;
    renderOnce();
  }

  function applyColors(colors: AvatarColors) {
    avatarGroup.traverse((o) => {
      if (!o.isMesh) return;
      const mats = Array.isArray(o.material) ? o.material : [o.material];
      for (const m of mats) {
        if (!m) continue;
        const name = (m.name || "").toLowerCase();
        if (colors.skin && /skin/.test(name)) m.color?.copy(colors.skin);
        else if (colors.hair && /hair/.test(name)) m.color?.copy(colors.hair);
      }
    });
  }

  function mattify(root: THREE.Object3D) {
    root.traverse((o) => {
      if (!o.isMesh) return;
      const mats = Array.isArray(o.material) ? o.material : [o.material];
      for (const m of mats) {
        if (!m) continue;
        if ("metalness" in m) m.metalness = 0;
        if ("roughness" in m) m.roughness = 1;
        m.needsUpdate = true;
      }
    });
  }

  async function loadEntityGlb(entity: Entity, bodyShape: string): Promise<THREE.Object3D | null> {
    const main = representationMainFile(entity, bodyShape);
    if (!main) return null;
    const fileMap = fileMapFor(entity);
    const mainBase = main.split("/").pop();
    const mainHash =
      fileMap.get(main.toLowerCase()) ??
      (mainBase !== undefined ? fileMap.get(mainBase.toLowerCase()) : undefined);
    if (!mainHash) return null;
    const manager = new THREE.LoadingManager();
    manager.setURLModifier((url) => {
      if (/^(blob:|data:)/.test(url)) return url;
      const path = url.split("?")[0] ?? url;
      const baseName = (path.split("/").pop() ?? "").toLowerCase();
      const hash = fileMap.get(baseName);
      return hash ? `${base}/content/contents/${hash}` : url;
    });
    const gltf = await new GLTFLoader(manager).loadAsync(`${base}/content/contents/${mainHash}`);
    return gltf.scene;
  }

  async function loadFacialFeature(
    entity: Entity,
    bodyShape: string,
  ): Promise<FacialFeature | null> {
    const names = representationContents(entity, bodyShape);
    const fileMap = fileMapFor(entity);
    const hashOf = (n: string | undefined): string | null =>
      n ? (fileMap.get(n) ?? fileMap.get(n.split("/").pop() ?? "") ?? null) : null;
    const texHash = hashOf(names.find((n) => n.endsWith(".png") && !n.endsWith("_mask.png")));
    if (!texHash) return null;
    const loadTex = async (hash: string, srgb: boolean): Promise<THREE.Texture> => {
      const t = await new THREE.TextureLoader().loadAsync(`${base}/content/contents/${hash}`);
      t.flipY = false;
      if (srgb) t.colorSpace = THREE.SRGBColorSpace;
      t.needsUpdate = true;
      return t;
    };
    const maskHash = hashOf(names.find((n) => n.endsWith("_mask.png")));
    return {
      tex: await loadTex(texHash, true),
      mask: maskHash ? await loadTex(maskHash, false) : null,
    };
  }

  function trimPartCache(): void {
    for (const [key, pending] of partCache) {
      if (partCache.size <= PART_CACHE_MAX) break;
      if (mounted.has(key)) continue;
      partCache.delete(key);
      void pending.then((obj) => {
        if (obj && obj.parent === null) disposeObject(obj);
      });
    }
  }

  function cachedPart(key: string, entity: Entity, bodyShape: string): Promise<THREE.Object3D | null> {
    let pending = partCache.get(key);
    if (pending) {
      partCache.delete(key);
      partCache.set(key, pending);
      return pending;
    }
    pending = withTimeout(loadEntityGlb(entity, bodyShape), GLB_TIMEOUT_MS)
      .then((obj) => {
        if (obj) mattify(obj);
        return obj;
      })
      .catch((err) => {
        console.warn("[wearable-preview] failed", key, err);
        partCache.delete(key);
        return null;
      });
    partCache.set(key, pending);
    trimPartCache();
    return pending;
  }

  function cachedFeature(key: string, entity: Entity, bodyShape: string): Promise<FacialFeature | null> {
    let pending = featureCache.get(key);
    if (!pending) {
      pending = loadFacialFeature(entity, bodyShape).catch((err) => {
        console.warn("[wearable-preview] failed", key, err);
        featureCache.delete(key);
        return null;
      });
      featureCache.set(key, pending);
    }
    return pending;
  }

  function hideBaseMeshes(root: THREE.Object3D, rules: HidingRules): void {
    root.traverse((o) => {
      if (!o.isMesh) return;
      const names = [o.name, o.parent?.name]
        .filter((s): s is string => Boolean(s))
        .map((s) => s.toLowerCase());
      o.visible = !baseMeshHidden(names, rules);
    });
  }

  function applyFacialFeatures(
    root: THREE.Object3D,
    features: Map<string, FacialFeature>,
    colors: AvatarColors,
    hidden: Set<string>,
  ): void {
    const SLOTS: [string, string, THREE.Color | null, THREE.Color | null][] = [
      ["mask_eyes", "eyes", colors.eyes, null],
      ["mask_eyebrows", "eyebrows", colors.hair, colors.hair],
      ["mask_mouth", "mouth", colors.skin, colors.skin],
    ];
    root.traverse((o) => {
      if (!(o as THREE.Mesh).isMesh) return;
      const mesh = o as THREE.Mesh;
      const names = [mesh.name, mesh.parent?.name]
        .filter((s): s is string => Boolean(s))
        .map((s) => s.toLowerCase());
      for (const [suffix, cat, maskTint, plainTint] of SLOTS) {
        if (!names.some((n) => n.endsWith(suffix))) continue;
        if (hidden.has(cat)) break;
        const feat = features.get(cat);
        if (!feat) {
          mesh.visible = false;
          break;
        }
        const mat = new THREE.MeshStandardMaterial({
          name: `feature_${cat}`,
          map: feat.tex,
          transparent: true,
          roughness: 1,
          metalness: 0,
        });
        if (feat.mask) {
          mat.emissiveMap = feat.mask;
          mat.emissive = new THREE.Color(0xffffff);
          if (maskTint) mat.color.copy(maskTint);
        } else if (plainTint) {
          mat.color.copy(plainTint);
        }
        const prev = Array.isArray(mesh.material) ? null : mesh.material;
        if (prev && String(prev.name || "").startsWith("feature_")) prev.dispose?.();
        mesh.material = mat;
        break;
      }
    });
  }

  function emoteUrlFor(e: string | null | undefined): string | null {
    if (!e) return null;
    if (/^https?:\/\//.test(e) || e.startsWith("/") || e.startsWith("blob:")) return e;
    return EMOTES[e] || null;
  }

  const clipCache = new Map<string, Promise<THREE.AnimationClip | null>>();

  function loadClip(url: string): Promise<THREE.AnimationClip | null> {
    let pending = clipCache.get(url);
    if (!pending) {
      pending = new GLTFLoader()
        .loadAsync(url)
        .then((gltf) => {
          const clips = gltf.animations || [];
          return (
            clips.find((c) => c.tracks.some((t) => t.name.startsWith("Avatar_"))) ||
            clips.slice().sort((a, b) => b.duration - a.duration)[0] ||
            null
          );
        })
        .catch((err) => {
          console.warn("[wearable-preview] emote failed", err);
          return null;
        });
      clipCache.set(url, pending);
    }
    return pending;
  }

  function clipBinds(clip: THREE.AnimationClip): boolean {
    const root = bodyRoot ?? mounted.values().next().value?.obj;
    if (!root) return true;
    return clip.tracks.some((t) => {
      try {
        const node = THREE.PropertyBinding.parseTrackName(t.name).nodeName;
        return Boolean(node) && root.getObjectByName(node) !== undefined;
      } catch {
        return false;
      }
    });
  }

  function bindClip(part: Part, clip: THREE.AnimationClip, at: number): THREE.AnimationMixer {
    const mixer = new THREE.AnimationMixer(part.obj);
    mixer.clipAction(clip).play();
    mixer.setTime(at);
    return mixer;
  }

  function applyClip(clip: THREE.AnimationClip): void {
    for (const m of mixers.values()) m.stopAllAction();
    mixers.clear();
    currentClip = clip;
    for (const part of mounted.values()) mixers.set(part.key, bindClip(part, clip, 0));
    lastFrame = performance.now();
    if (!revealed) frame();
    reveal();
    renderOnce();
  }

  async function playEmote(url: string): Promise<boolean> {
    let clip = await loadClip(url);
    if (disposed) return false;
    if (clip && !clipBinds(clip)) {
      console.warn("[wearable-preview] emote does not bind to this avatar", url);
      clip = null;
    }
    if (!clip && url !== EMOTES.idle) clip = await loadClip(EMOTES.idle as string);
    if (!clip || disposed) return false;
    applyClip(clip);
    return true;
  }

  async function setEmote(input: string | null | undefined): Promise<void> {
    const url = emoteUrlFor(input);
    if (!url || disposed || !mounted.size) return;
    cycleGen++;
    await playEmote(url);
  }

  const EMOTE_REST_MS = 1500;
  let cycleGen = 0;
  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

  async function runEmoteCycle(inputs: string[]): Promise<void> {
    const gen = ++cycleGen;
    const urls = inputs
      .map(emoteUrlFor)
      .filter((u): u is string => Boolean(u));
    if (!urls.length) return;
    for (let i = 0; !disposed && gen === cycleGen; i++) {
      const clip = await loadClip(urls[i % urls.length] as string);
      if (disposed || gen !== cycleGen) return;
      if (clip && clipBinds(clip)) {
        applyClip(clip);
        await sleep(clip.duration * 1000);
        if (disposed || gen !== cycleGen) return;
      }
      const idleClip = await loadClip(EMOTES.idle as string);
      if (disposed || gen !== cycleGen) return;
      if (idleClip) applyClip(idleClip);
      await sleep(EMOTE_REST_MS);
    }
  }

  async function bindInitialEmote(): Promise<void> {
    const wanted = opts.emotes?.length ? opts.emotes[0] : opts.emote;
    await playEmote(emoteUrlFor(wanted) ?? (EMOTES.idle as string));
    if (disposed) return;
    reveal();
    if (opts.emotes?.length) void runEmoteCycle(opts.emotes);
  }

  async function reconcile(target: ResolvedOutfit, gen: number): Promise<{ count: number; bodyChanged: boolean } | null> {
    const { bodyShape, wearables } = target;
    const color = (c: { r: number; g: number; b: number } | null) => c ? new THREE.Color(c.r, c.g, c.b) : null;
    const colors: AvatarColors = { skin: color(target.colors.skin), hair: color(target.colors.hair), eyes: color(target.colors.eyes) };
    const bodyLc = bodyShape.toLowerCase();
    const byPointer = await fetchEntities(base, [bodyShape, ...wearables]);
    if (stale(gen)) return null;
    const rules = computeHiding(wearables, byPointer);

    const wantParts: { key: string; urn: string; isBody: boolean; entity: Entity }[] = [];
    const wantFeatures: { key: string; cat: string; entity: Entity }[] = [];
    const bodyEntity = byPointer.get(bodyLc);
    if (bodyEntity)
      wantParts.push({ key: partKey(bodyLc, bodyLc), urn: bodyShape, isBody: true, entity: bodyEntity });
    for (const urn of wearables) {
      const e = byPointer.get(urn.toLowerCase());
      if (!e) continue;
      const cat = categoryOf(e);
      if (cat === "body_shape") continue;
      if (cat !== null && rules.hidden.has(cat)) continue;
      if (cat !== null && FACIAL_CATS.has(cat))
        wantFeatures.push({ key: partKey(urn, bodyLc), cat, entity: e });
      else wantParts.push({ key: partKey(urn, bodyLc), urn, isBody: false, entity: e });
    }

    const [objs, feats] = await Promise.all([
      Promise.all(wantParts.map((p) => cachedPart(p.key, p.entity, bodyShape))),
      Promise.all(wantFeatures.map((f) => cachedFeature(f.key, f.entity, bodyShape))),
    ]);
    if (stale(gen)) return null;

    const at = mixers.values().next().value?.time ?? 0;
    const nextKeys = new Set(wantParts.map((p) => p.key));
    for (const [key, part] of mounted) {
      if (nextKeys.has(key)) continue;
      mixers.get(key)?.stopAllAction();
      mixers.delete(key);
      avatarGroup.remove(part.obj);
      mounted.delete(key);
    }
    let nextBody: THREE.Object3D | null = null;
    for (let i = 0; i < wantParts.length; i++) {
      const p = wantParts[i] as (typeof wantParts)[number];
      const obj = objs[i];
      if (!obj) continue;
      if (p.isBody) nextBody = obj;
      if (mounted.has(p.key)) continue;
      const part: Part = { key: p.key, urn: p.urn, isBody: p.isBody, obj };
      if (currentClip) mixers.set(p.key, bindClip(part, currentClip, at));
      avatarGroup.add(obj);
      mounted.set(p.key, part);
    }
    const features = new Map<string, FacialFeature>();
    for (let i = 0; i < wantFeatures.length; i++) {
      const f = wantFeatures[i] as (typeof wantFeatures)[number];
      const feat = feats[i];
      if (feat) features.set(f.cat, feat);
    }
    const bodyChanged = nextBody !== bodyRoot;
    bodyRoot = nextBody;
    if (bodyRoot) {
      hideBaseMeshes(bodyRoot, rules);
      applyFacialFeatures(bodyRoot, features, colors, rules.hidden);
    }
    applyColors(colors);
    renderOnce();
    return { count: mounted.size, bodyChanged };
  }

  function failInitial(err: unknown): void {
    console.error("[wearable-preview]", err);
    scene.visible = false;
    setStatus("error");
  }

  async function refresh(gen: number): Promise<void> {
    const initial = !revealed;
    if (initial) {
      scene.visible = true;
      if (status === "error") setStatus("loading");
    }
    try {
      const target = await resolveOutfit(base, opts);
      if (stale(gen)) return;
      const result = await reconcile(target, gen);
      if (!result) return;
      if (!result.count) {
        scene.visible = false;
        setStatus("empty");
        return;
      }
      if (initial || result.bodyChanged) frame();
      if (initial) {
        await bindInitialEmote();
        if (stale(gen)) return;
      }
      scene.visible = true;
      renderOnce();
      setStatus(result.count ? "ready" : "empty");
    } catch (err) {
      if (stale(gen)) return;
      if (initial) failInitial(err);
      else console.warn("[wearable-preview] outfit swap failed", err);
    }
  }

  function settleSwap(): void {
    if (swapTimer) clearTimeout(swapTimer);
    swapTimer = null;
    const settle = swapSettle;
    swapSettle = null;
    settle?.();
  }

  function setOutfit(next: AvatarOutfitOptions): Promise<void> {
    if (disposed || opts.model) return Promise.resolve();
    Object.assign(opts, next);
    const gen = ++loadGen;
    settleSwap();
    return new Promise((resolve) => {
      swapSettle = resolve;
      swapTimer = setTimeout(() => {
        swapTimer = null;
        swapSettle = null;
        if (stale(gen)) return resolve();
        void refresh(gen).then(resolve);
      }, OUTFIT_SWAP_DEBOUNCE_MS);
    });
  }

  async function loadModel(url: string): Promise<void> {
    try {
      setStatus("loading");
      const s = (await new GLTFLoader().loadAsync(url)).scene;
      if (disposed) return;
      const part: Part = { key: `model|${url}`, urn: url, isBody: true, obj: s };
      mattify(s);
      avatarGroup.add(s);
      mounted.set(part.key, part);
      bodyRoot = s;
      frame();
      if (opts.emotes?.length) void runEmoteCycle(opts.emotes);
      else {
        const me = emoteUrlFor(opts.emote);
        if (me) await playEmote(me);
      }
      reveal();
      setStatus("ready");
    } catch (err) {
      if (!disposed) failInitial(err);
    }
  }

  setStatus("loading");
  if (opts.model) void loadModel(opts.model);
  else void refresh(++loadGen);

  function dispose() {
    disposed = true;
    cycleGen++;
    loadGen++;
    settleSwap();
    if (demandIdleTimer) clearTimeout(demandIdleTimer);
    renderer.setAnimationLoop(null);
    for (const m of mixers.values()) m.stopAllAction();
    mixers.clear();
    controls.dispose();
    scene.traverse((o) => {
      if (o.geometry) o.geometry.dispose?.();
      const mats = o.material ? (Array.isArray(o.material) ? o.material : [o.material]) : [];
      for (const m of mats) {
        for (const k in m) {
          const v = m[k];
          if (isTexture(v)) v.dispose?.();
        }
        m.dispose?.();
      }
    });
    for (const pending of partCache.values())
      void pending.then((obj) => {
        if (obj && obj.parent === null) disposeObject(obj);
      });
    partCache.clear();
    for (const pending of featureCache.values())
      void pending.then((feat) => {
        feat?.tex.dispose();
        feat?.mask?.dispose();
      });
    featureCache.clear();
    renderer.dispose();
    renderer.forceContextLoss();
    if (renderer.domElement.parentNode === container) container.removeChild(renderer.domElement);
  }

  function setCamera(next: AvatarCameraOptions): void {
    Object.assign(opts, next);
    camera.fov = opts.fov ?? 34;
    frame();
  }

  return { resize, dispose, setActive, setEmote, setCamera, setOutfit };
}
