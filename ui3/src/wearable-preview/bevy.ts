import type { AvatarScene, AvatarSceneOptions, AvatarOutfitOptions, AvatarCameraOptions } from "./avatar";
import type { BevyPreviewSession } from "./bevy-host";
import { resolveOutfit } from "./outfit";

export function createAvatarScene(container: HTMLElement, options: AvatarSceneOptions): AvatarScene {
  const host = window.dclAvatarPreview;
  if (!host) throw new Error("Bevy preview host is unavailable");
  const canvas = document.createElement("canvas");
  canvas.style.cssText = "width:100%;height:100%;display:block;touch-action:none;object-fit:contain";
  canvas.dataset.renderer = "bevy";
  container.append(canvas);
  const context = canvas.getContext("bitmaprenderer");
  if (!context) { canvas.remove(); throw new Error("Image bitmap presentation is unavailable"); }
  let disposed = false;
  let generation = 0;
  let readyGeneration = 0;
  let requestedSize: { width: number; height: number } | undefined;
  let session: BevyPreviewSession | undefined;
  let outfit: AvatarOutfitOptions = options;
  let camera: AvatarCameraOptions = { ...options };
  let active = true;
  let emote = options.emote;
  let cycle = 0;
  let timer: ReturnType<typeof setInterval> | undefined;
  const reducedMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
  const base = (options.base || location.origin).replace(/\/$/, "");
  const rgb = (color: {r: number; g: number; b: number} | null) => color ? [color.r, color.g, color.b] : null;
  const wake = () => { if (reducedMotion) session?.send({ op: "active", active: active && !document.hidden }); };
  const sendCamera = () => { session?.send({ op: "camera", zoom: camera.zoom, yaw: (camera.yaw ?? 0) * Math.PI / 180,
    pitch: (camera.pitch ?? 0) * Math.PI / 180, fov: camera.fov, targetY: camera.targetY,
    spin: (options.spin ?? true) && !reducedMotion, spinSpeed: options.spinSpeed ?? 0.25 }); wake(); };
  const resize = () => {
    const rect = container.getBoundingClientRect();
    const scale = Math.min(2, 768 / Math.max(rect.width,1), 1024 / Math.max(rect.height,1));
    requestedSize = { width: Math.max(16,Math.round(rect.width * scale)), height: Math.max(16,Math.round(rect.height * scale)) };
    session?.send({ op: "resize", ...requestedSize });
    wake();
  };
  const play = () => {
    const value = emote || "idle";
    const urn = value.startsWith("urn:") ? value : `urn:decentraland:off-chain:base-emotes:${value}`;
    session?.send({ op: "emote", emote: urn });
    wake();
  };
  const setOutfit = async (next: AvatarOutfitOptions) => {
    outfit = next;
    if (!session) return;
    const revision = ++generation;
    options.onStatus?.("loading");
    try {
      const resolved = await resolveOutfit(base, next);
      if (disposed || revision !== generation) return;
      session.send({ op: "outfit", generation: revision, bodyShape: resolved.bodyShape, wearables: resolved.wearables,
        skin: rgb(resolved.colors.skin), hair: rgb(resolved.colors.hair), eyes: rgb(resolved.colors.eyes) });
      sendCamera(); resize(); play();
      session.send({ op: "active", active: active && !document.hidden });
    } catch (error) {
      if (disposed || revision !== generation) return;
      console.error("[Bevy avatar preview]", error);
      options.onStatus?.("error");
    }
  };
  void host.create((bitmap, revision) => {
    if (disposed || revision !== generation || (requestedSize && (bitmap.width !== requestedSize.width || bitmap.height !== requestedSize.height))) { bitmap.close(); return; }
    canvas.width = bitmap.width; canvas.height = bitmap.height;
    context.transferFromImageBitmap(bitmap);
    if (readyGeneration !== revision) {
      readyGeneration = revision;
      options.onStatus?.("ready");
    }
    if (reducedMotion) session?.send({ op: "active", active: false });
  }).then(async (created) => {
    if (disposed) { created.dispose(); return; }
    session = created;
    await setOutfit(outfit);
    if (disposed || reducedMotion || !options.emotes?.length) return;
    emote = options.emotes[0]; play();
    timer = setInterval(() => {
      if (!active) return;
      cycle = (cycle + 1) % options.emotes!.length;
      emote = options.emotes![cycle]; play();
    }, 6000);
  }).catch(error => {
    if (!disposed) { console.error("[Bevy avatar preview]", error); options.onStatus?.("error"); }
  });
  let drag: { x: number; y: number } | undefined;
  const down = (event: PointerEvent) => {
    if (options.controls === false) return;
    drag = { x: event.clientX, y: event.clientY };
    canvas.setPointerCapture(event.pointerId);
  };
  const move = (event: PointerEvent) => {
    if (!drag) return;
    camera.yaw = (camera.yaw ?? 0) + (event.clientX-drag.x) * 0.5;
    camera.pitch = Math.max(-65,Math.min(65,(camera.pitch ?? 0) + (event.clientY-drag.y) * 0.3));
    drag = { x: event.clientX, y: event.clientY }; sendCamera();
    session?.send({ op: "camera", spin: false });
  };
  const up = () => { drag = undefined; };
  const wheel = (event: WheelEvent) => {
    if (options.controls === false) return;
    event.preventDefault();
    camera.zoom = Math.max(0.3,Math.min(5,(camera.zoom ?? 1) * Math.exp(-event.deltaY * 0.001)));
    sendCamera();
  };
  canvas.addEventListener("pointerdown", down);
  canvas.addEventListener("pointermove", move);
  canvas.addEventListener("pointerup", up);
  canvas.addEventListener("pointercancel", up);
  canvas.addEventListener("wheel", wheel, { passive: false });
  const visibility = () => session?.send({ op: "active", active: active && !document.hidden });
  document.addEventListener("visibilitychange", visibility);
  return {
    resize,
    setOutfit,
    setActive(value) { active = value; visibility(); },
    async setEmote(value) { emote = value ?? undefined; play(); },
    setCamera(value) { camera = { ...camera, ...value }; sendCamera(); },
    dispose() {
      disposed = true; generation++; clearInterval(timer); session?.dispose();
      document.removeEventListener("visibilitychange", visibility);
      canvas.remove();
    },
  };
}
