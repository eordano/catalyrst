import * as THREE from "three";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";

const idleUrl = new URL("./emotes/idle.glb", import.meta.url).href;
const waveUrl = new URL("./emotes/wave.glb", import.meta.url).href;
const danceUrl = new URL("./emotes/dance.glb", import.meta.url).href;
const clapUrl = new URL("./emotes/clap.glb", import.meta.url).href;
const dabUrl = new URL("./emotes/dab.glb", import.meta.url).href;

// THREE.Cache is deliberately left OFF. Some hosts mount several stages
// at once -- each its own WebGLRenderer, i.e. its own WebGL context -- and many
// share a base outfit. With the cache on, THREE hands every one of them the *same*
// decoded image/texture object for a shared content URL; a texture belongs to the
// one context that first uploads it, so the others render black and spam
// "Texture marked for update but no image data found" every frame. The
// content-addressed GLB/PNG files are immutable, so the browser HTTP cache
// still serves the refetch from disk -- we lose a re-decode, not a round
// trip. Entity JSON is deduped separately (fetchActiveEntities), which is
// safe because it is not a per-context GPU object.

function prefersReducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

export type AvatarStatus = "loading" | "ready" | "empty" | "error";

interface ColorWrapper {
  color?: { r: number; g: number; b: number };
}

interface OutfitData {
  address?: string;
  slot?: number;
  bodyShape?: string;
  wearables?: string[];
  skin?: ColorWrapper;
  hair?: ColorWrapper;
  eyes?: ColorWrapper;
}

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

export interface AvatarScene {
  resize: () => void;
  dispose: () => void;
  /** Pause (false) or resume (true) the render loop -- an offscreen tile stops
   *  spinning and rendering at refresh rate until it scrolls back into view. */
  setActive: (active: boolean) => void;
  setEmote: (input: string | null | undefined) => Promise<void>;
  setCamera: (next: AvatarCameraOptions) => void;
}

type AvatarColors = {
  skin: THREE.Color | null;
  hair: THREE.Color | null;
  eyes: THREE.Color | null;
};

interface Representation {
  bodyShapes?: string[];
  mainFile?: string;
  contents?: string[];
}

interface EntityData {
  category?: string;
  representations?: Representation[];
  hides?: string[];
  replaces?: string[];
  removesDefaultHiding?: string[];
}

interface Entity {
  pointers?: string[];
  content?: { file: string; hash: string }[];
  metadata?: { data?: EntityData };
}

interface Avatar {
  bodyShape?: string;
  wearables?: string[];
  skin?: ColorWrapper;
  hair?: ColorWrapper;
  eyes?: ColorWrapper;
}

interface ProfileEnvelope {
  avatars?: { avatar?: Avatar }[];
}

interface OutfitSlot {
  slot?: number;
  outfit?: OutfitData;
}

interface OutfitsEnvelope {
  metadata?: { outfits?: OutfitSlot[] };
  outfits?: OutfitSlot[];
}

const EMOTES: Record<string, string> = {
  idle: idleUrl,
  wave: waveUrl,
  dance: danceUrl,
  clap: clapUrl,
  dab: dabUrl,
};

const DEFAULT_BASE = "https://catalyst.example.com";
const DEFAULT_BODY = "urn:decentraland:off-chain:base-avatars:BaseMale";

// Shipped in the /play overlay (client-only build), so the served origin -- which
// also fronts the catalyst API -- is the portable default. DEFAULT_BASE survives
// only in SSR/non-browser bundles; import.meta.env.SSR folds it out of the client
// build, leaving no baked host.
function defaultBase(): string {
  return import.meta.env.SSR ? DEFAULT_BASE : window.location.origin;
}

const itemUrn = (urn: string): string => {
  const p = urn.split(":");
  return p.length === 7 && p[3] !== undefined && /^collections-v[12]$/.test(p[3])
    ? p.slice(0, 6).join(":")
    : urn;
};

const colorOf = (c: ColorWrapper | null | undefined): THREE.Color | null =>
  c && c.color && typeof c.color.r === "number"
    ? new THREE.Color(c.color.r, c.color.g, c.color.b)
    : null;

function isTexture(value: unknown): value is { isTexture: unknown; dispose?: () => void } {
  return (
    typeof value === "object" &&
    value !== null &&
    "isTexture" in value &&
    Boolean((value as { isTexture?: unknown }).isTexture)
  );
}

async function getJSON<T>(url: string, opts?: RequestInit): Promise<T> {
  const r = await fetch(url, opts);
  if (!r.ok) throw new Error(`${url} -> ${r.status}`);
  return r.json() as Promise<T>;
}

// The active-entity lookup for a pointer set is a pure, content-addressed query,
// so several tiles that show the same base outfit would otherwise each POST it.
// Dedupe on the (base, sorted pointers) key: fetched once per page, shared by
// every stage. A failed lookup is dropped from the cache so a later tile can retry.
const activeEntitiesCache = new Map<string, Promise<Entity[]>>();
function fetchActiveEntities(base: string, pointers: string[]): Promise<Entity[]> {
  const key = `${base}|${[...pointers].sort().join(",")}`;
  let pending = activeEntitiesCache.get(key);
  if (!pending) {
    pending = getJSON<Entity[]>(`${base}/content/entities/active`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ pointers }),
    }).catch((err) => {
      activeEntitiesCache.delete(key);
      throw err;
    });
    activeEntitiesCache.set(key, pending);
  }
  return pending;
}

function representationMainFile(entity: Entity, bodyShape: string): string | null {
  const reps = entity?.metadata?.data?.representations || [];
  const bs = bodyShape.toLowerCase();
  const rep =
    reps.find((r) => (r.bodyShapes || []).some((b) => String(b).toLowerCase() === bs)) || reps[0];
  return rep?.mainFile || null;
}

function fileMapFor(entity: Entity): Map<string, string> {
  const map = new Map<string, string>();
  for (const c of entity.content || []) {
    const f = String(c.file).toLowerCase();
    map.set(f, c.hash);
    const last = f.split("/").pop();
    if (last !== undefined) map.set(last, c.hash);
  }
  return map;
}

export function createAvatarScene(
  container: HTMLElement,
  opts: AvatarSceneOptions = {},
): AvatarScene {
  const base = (opts.base || defaultBase()).replace(/\/$/, "");
  const setStatus: (status: AvatarStatus) => void = opts.onStatus || (() => {});
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

  // WCAG 2.2.2: the CSS kill-switch never reaches WebGL, so honor the media
  // query here -- no sway and mixers held at their first keyframe (a posed
  // still); drag-orbit stays live because controls keep updating.
  const reducedMotion = prefersReducedMotion();
  const sway = opts.spin !== false && !reducedMotion;
  const swayAmplitude = THREE.MathUtils.degToRad(60);
  const swaySpeed = (opts.spinSpeed ?? 0.9) * 0.6;
  let swayT = 0;

  const avatarGroup = new THREE.Group();
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

  const parts: THREE.Object3D[] = [];
  const mixers: THREE.AnimationMixer[] = [];
  let lastFrame = performance.now();
  // Declared before the initial resize() call below, which -- under reduced
  // motion -- reaches renderOnce() and so reads `active` immediately.
  let active = true;

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
    for (const m of mixers) m.update(reducedMotion ? 0 : dt);
    if (sway) {
      swayT += dt;
      avatarGroup.rotation.y = swayAmplitude * Math.sin(swayT * swaySpeed);
    }
    controls.update();
    renderer.render(scene, camera);
  }
  if (!reducedMotion) renderer.setAnimationLoop(renderFrame);

  // Reduced motion: no continuous rAF loop (renderFrame above never free-runs).
  // A frame is drawn only for discrete events -- a pose settling during load,
  // an emote swap, a resize, a camera move -- via renderOnce(), plus a
  // temporary loop for as long as OrbitControls is actually being dragged or
  // still decelerating from damping. Composes with setActive: renderOnce and
  // the demand loop are both no-ops while paused offscreen.
  let demandLoopActive = false;
  let demandIdleTimer: ReturnType<typeof setTimeout> | null = null;
  // ~2 damping time-constants (velocity decays by dampingFactor each frame).
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

  // Pushes the demand loop's stop out by one damping settle window; called on
  // every 'change' while dragging/damping and once on 'end', so the loop keeps
  // running until activity actually stops rather than a fixed time after 'end'.
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
    // Reset the frame clock so a resumed loop doesn't spend the whole paused
    // interval as one dt (which would snap the sway and any playing emote).
    lastFrame = performance.now();
    if (reducedMotion) {
      if (next) renderOnce();
      else stopDemandLoop();
    } else {
      renderer.setAnimationLoop(next ? renderFrame : null);
    }
  }

  function frame() {
    const box = new THREE.Box3().setFromObject(avatarGroup);
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

  function mattify() {
    avatarGroup.traverse((o) => {
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

  async function resolveAvatar(): Promise<{
    bodyShape: string;
    wearables: string[];
    colors: AvatarColors;
  }> {
    let bodyShape = opts.body || DEFAULT_BODY;
    let wearables = Array.isArray(opts.urns)
      ? opts.urns.slice()
      : String(opts.urns || "")
          .split(",")
          .map((s) => s.trim())
          .filter(Boolean);
    let colors: AvatarColors = { skin: null, hair: null, eyes: null };

    let outfit: OutfitData | null =
      opts.outfit && typeof opts.outfit === "object" ? opts.outfit : null;
    if (outfit && outfit.address) {
      try {
        const env = await getJSON<OutfitsEnvelope>(
          `${base}/lambdas/outfits/${String(outfit.address).toLowerCase()}`,
        );
        const list = env?.metadata?.outfits || env?.outfits || [];
        const slot = outfit.slot ?? 0;
        outfit = (list.find((o) => o.slot === slot) || list[0])?.outfit || null;
      } catch {
        outfit = null;
      }
    }

    if (outfit && (Array.isArray(outfit.wearables) || outfit.bodyShape)) {
      if (outfit.bodyShape) bodyShape = outfit.bodyShape;
      if (Array.isArray(outfit.wearables)) wearables = outfit.wearables;
      colors = {
        skin: colorOf(outfit.skin),
        hair: colorOf(outfit.hair),
        eyes: colorOf(outfit.eyes),
      };
    } else if (opts.profile) {
      const env = await getJSON<ProfileEnvelope>(
        `${base}/lambdas/profile/${String(opts.profile).toLowerCase()}`,
      );
      const av: Avatar = (env.avatars || [])[0]?.avatar || {};
      if (av.bodyShape) bodyShape = av.bodyShape;
      if (Array.isArray(av.wearables) && av.wearables.length) wearables = av.wearables;
      colors = { skin: colorOf(av.skin), hair: colorOf(av.hair), eyes: colorOf(av.eyes) };
    }

    const seen = new Set<string>();
    wearables = wearables.map(itemUrn).filter((u) => (seen.has(u) ? false : (seen.add(u), true)));
    return { bodyShape, wearables, colors };
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

  type FacialFeature = { tex: THREE.Texture; mask: THREE.Texture | null };
  const FACIAL_CATS = new Set(["eyes", "eyebrows", "mouth"]);

  async function loadFacialFeature(
    entity: Entity,
    bodyShape: string,
  ): Promise<FacialFeature | null> {
    const reps = entity?.metadata?.data?.representations || [];
    const bs = bodyShape.toLowerCase();
    const rep =
      reps.find((r) => (r.bodyShapes || []).some((b) => String(b).toLowerCase() === bs)) ||
      reps[0];
    const names = (
      rep?.contents?.length ? rep.contents : (entity.content || []).map((c) => c.file)
    ).map((n) => String(n).toLowerCase());
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

  async function setEmote(input: string | null | undefined): Promise<void> {
    const url = emoteUrlFor(input);
    if (!url || disposed || !parts.length) return;
    cycleGen++;
    await playEmote(url);
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

  function applyClip(clip: THREE.AnimationClip): void {
    for (const m of mixers) m.stopAllAction();
    mixers.length = 0;
    for (const part of parts) {
      const mixer = new THREE.AnimationMixer(part);
      mixer.clipAction(clip).play();
      mixers.push(mixer);
    }
    lastFrame = performance.now();
    renderOnce();
  }

  async function playEmote(url: string): Promise<void> {
    const clip = await loadClip(url);
    if (clip && !disposed) applyClip(clip);
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
      if (clip) {
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

  async function load(): Promise<void> {
    try {
      if (opts.model) {
        setStatus("loading");
        const s = (await new GLTFLoader().loadAsync(opts.model)).scene;
        avatarGroup.add(s);
        parts.push(s);
        mattify();
        frame();
        if (opts.emotes?.length) void runEmoteCycle(opts.emotes);
        else {
          const me = emoteUrlFor(opts.emote);
          if (me) await playEmote(me);
        }
        setStatus("ready");
        return;
      }
      setStatus("loading");
      const { bodyShape, wearables, colors } = await resolveAvatar();
      if (disposed) return;
      const pointers = [bodyShape, ...wearables];
      const entities = await fetchActiveEntities(base, pointers);
      if (disposed) return;
      const byPointer = new Map<string, Entity>();
      for (const e of entities)
        for (const p of e.pointers || []) byPointer.set(String(p).toLowerCase(), e);

      const bodyLc = String(bodyShape).toLowerCase();
      const catOf = (e: Entity | undefined): string | null => e?.metadata?.data?.category || null;
      const equippedCats = new Set<string>();
      const hidden = new Set<string>();
      let skinEquipped = false;
      let handsDefaultHidden = false;
      for (const urn of wearables) {
        const e = byPointer.get(urn.toLowerCase());
        if (!e) continue;
        const cat = catOf(e);
        if (!cat) continue;
        equippedCats.add(cat);
        if (cat === "skin") skinEquipped = true;
        const d: EntityData = e.metadata?.data || {};
        for (const h of [...(d.hides || []), ...(d.replaces || [])]) if (h !== cat) hidden.add(h);
        const coversUpperBody = cat === "upper_body" || (d.hides || []).includes("upper_body");
        if (coversUpperBody && !(d.removesDefaultHiding || []).includes("hands"))
          handsDefaultHidden = true;
      }
      if (skinEquipped)
        for (const c of [
          "eyes", "mouth", "eyebrows", "hair", "facial_hair",
          "upper_body", "lower_body", "feet", "hands_wear", "hands", "head",
        ])
          hidden.add(c);

      const HIDERS: [string, () => boolean][] = [
        ["ubody_basemesh", () => equippedCats.has("upper_body") || hidden.has("upper_body")],
        ["lbody_basemesh", () => equippedCats.has("lower_body") || hidden.has("lower_body")],
        ["feet_basemesh", () => equippedCats.has("feet") || hidden.has("feet")],
        ["hands_basemesh", () => equippedCats.has("hands_wear") || hidden.has("hands") || hidden.has("hands_wear") || handsDefaultHidden],
        ["head_basemesh", () => hidden.has("head")],
        ["mask_eyes", () => hidden.has("eyes")],
        ["mask_eyebrows", () => hidden.has("eyebrows")],
        ["mask_mouth", () => hidden.has("mouth")],
      ];
      const hideBaseMeshes = (root: THREE.Object3D) => {
        root.traverse((o) => {
          if (!o.isMesh) return;
          const names = [o.name, o.parent?.name]
            .filter((s): s is string => Boolean(s))
            .map((s) => s.toLowerCase());
          for (const [suffix, pred] of HIDERS) {
            if (names.some((n) => n.endsWith(suffix)) && (skinEquipped || pred())) {
              o.visible = false;
              break;
            }
          }
        });
      };

      let loaded = 0;
      let bodyRoot: THREE.Object3D | null = null;
      const features = new Map<string, FacialFeature>();
      await Promise.allSettled(
        pointers.map(async (urn) => {
          const e = byPointer.get(urn.toLowerCase());
          if (!e) return;
          const isBody = urn.toLowerCase() === bodyLc;
          const cat = catOf(e);
          if (!isBody && cat !== null && hidden.has(cat)) return;
          try {
            if (!isBody && cat !== null && FACIAL_CATS.has(cat)) {
              const feat = await loadFacialFeature(e, bodyShape);
              if (feat && !disposed) {
                features.set(cat, feat);
                loaded++;
              }
              return;
            }
            const obj = await Promise.race<THREE.Object3D | null>([
              loadEntityGlb(e, bodyShape),
              new Promise<THREE.Object3D | null>((_resolve, reject) =>
                setTimeout(() => reject(new Error("timeout")), 20000),
              ),
            ]);
            if (obj && !disposed) {
              if (isBody) {
                bodyRoot = obj;
                hideBaseMeshes(obj);
              }
              avatarGroup.add(obj);
              parts.push(obj);
              mattify();
              applyColors(colors);
              frame();
              loaded++;
            }
          } catch (err) {
            console.warn("[wearable-preview] failed", urn, err);
          }
        }),
      );
      if (disposed) return;
      if (bodyRoot) applyFacialFeatures(bodyRoot, features, colors, hidden);
      mattify();
      applyColors(colors);
      frame();

      if (opts.emotes?.length && parts.length) void runEmoteCycle(opts.emotes);
      else {
        const m = emoteUrlFor(opts.emote);
        if (m && parts.length) await playEmote(m);
      }

      setStatus(loaded ? "ready" : "empty");
    } catch (err) {
      console.error("[wearable-preview]", err);
      if (!disposed) {
        scene.visible = false;
        setStatus("error");
      }
    }
  }

  load();

  function dispose() {
    disposed = true;
    cycleGen++;
    if (demandIdleTimer) clearTimeout(demandIdleTimer);
    renderer.setAnimationLoop(null);
    for (const m of mixers) m.stopAllAction();
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
    renderer.dispose();
    // dispose() frees GL resources but the context itself lingers until GC;
    // browsers cap live contexts (~16), so a page cycling several previews can
    // silently kill its oldest canvases. Release the context deterministically.
    renderer.forceContextLoss();
    if (renderer.domElement.parentNode === container) container.removeChild(renderer.domElement);
  }

  function setCamera(next: AvatarCameraOptions): void {
    Object.assign(opts, next);
    camera.fov = opts.fov ?? 34;
    frame();
  }

  return { resize, dispose, setActive, setEmote, setCamera };
}
