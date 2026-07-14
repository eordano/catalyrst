import { useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import type { AvatarScene, AvatarSceneOptions, AvatarStatus } from "./avatar";

type WearablePreviewProps = AvatarSceneOptions & {
  emoteNonce?: number;
  className?: string;
  style?: CSSProperties;
  /** Stop the render loop while this preview is scrolled out of view, and resume
   *  it when it returns. Off by default so interactive previews keep animating. */
  pauseOffscreen?: boolean;
};

export default function WearablePreview({
  profile,
  urns,
  body,
  outfit,
  model,
  emote,
  emotes,
  emoteNonce = 0,
  base,
  zoom,
  yaw,
  pitch,
  fov,
  targetY,
  controls = true,
  pan = false,
  platform = false,
  spin = true,
  spinSpeed,
  background,
  className,
  style,
  pauseOffscreen = false,
  onStatus,
}: WearablePreviewProps) {
  const ref = useRef<HTMLDivElement | null>(null);
  const sceneRef = useRef<AvatarScene | null>(null);
  const visibleRef = useRef(true);
  const [status, setStatus] = useState<AvatarStatus>("loading");
  // Gated tiles (pauseOffscreen, with IO support) don't boot their scene -- and
  // so don't fetch a single GLB -- until the observer below reports them
  // actually visible. Everything else boots on mount exactly as before.
  const [booted, setBooted] = useState<boolean>(
    () => !(pauseOffscreen && typeof IntersectionObserver !== "undefined"),
  );

  // Camera props are applied to the live scene (setCamera) rather than keyed into
  // the scene-creating effect -- a zoom/yaw nudge must not tear down and reload GLBs.
  const cameraRef = useRef({ zoom, yaw, pitch, fov, targetY });
  cameraRef.current = { zoom, yaw, pitch, fov, targetY };

  const key = JSON.stringify([
    profile, Array.isArray(urns) ? urns : urns ?? null, body, outfit ?? null, model, base,
    controls, pan, platform, spin, spinSpeed, background,
    emotes ?? null,
  ]);

  useEffect(() => {
    if (!booted) return;
    const el = ref.current;
    if (!el) return;
    let scene: AvatarScene | null = null;
    let ro: ResizeObserver | null = null;
    let cancelled = false;
    setStatus("loading");

    import("./avatar")
      .then(({ createAvatarScene }) => {
        const node = ref.current;
        if (cancelled || !node) return;
        scene = createAvatarScene(node, {
          profile,
          urns,
          body,
          outfit,
          model,
          emote,
          emotes,
          base,
          ...cameraRef.current,
          controls,
          pan,
          platform,
          spin,
          spinSpeed,
          background,
          onStatus: (s) => {
            if (cancelled) return;
            setStatus(s);
            onStatus?.(s);
          },
        });
        sceneRef.current = scene;
        // Adopt the visibility the observer has already settled on, so a stage
        // created while off-screen starts paused instead of rendering once.
        scene.setActive(visibleRef.current);
        ro = new ResizeObserver(() => scene?.resize());
        ro.observe(node);
      })
      .catch((err) => {
        if (!cancelled) {
          console.error("[WearablePreview]", err);
          setStatus("error");
          onStatus?.("error");
        }
      });

    return () => {
      cancelled = true;
      if (ro) ro.disconnect();
      if (scene) scene.dispose();
      sceneRef.current = null;
    };
  }, [key, booted]);

  // Independent of the scene-creating effect (so toggling visibility never
  // tears down and reloads the GLBs): tracks visibility for the pause/resume
  // loop, and -- the first time the tile is actually seen -- flips `booted`,
  // which is what lets the effect above create the scene at all.
  useEffect(() => {
    if (!pauseOffscreen) return;
    const el = ref.current;
    if (!el || typeof IntersectionObserver === "undefined") return;
    const io = new IntersectionObserver(
      (entries) => {
        const visible = entries.some((e) => e.isIntersecting);
        visibleRef.current = visible;
        sceneRef.current?.setActive(visible);
        if (visible) setBooted(true);
      },
      { rootMargin: "120px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [pauseOffscreen]);

  useEffect(() => {
    sceneRef.current?.setEmote?.(emote);
  }, [emote, emoteNonce]);

  useEffect(() => {
    sceneRef.current?.setCamera?.(cameraRef.current);
  }, [zoom, yaw, pitch, fov, targetY]);

  return (
    <div
      ref={ref}
      className={className}
      data-status={status}
      style={{ width: "100%", height: "100%", position: "relative", ...style }}
    />
  );
}
