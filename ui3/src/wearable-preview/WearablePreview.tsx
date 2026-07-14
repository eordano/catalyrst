import { useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import type { AvatarScene, AvatarSceneOptions, AvatarStatus } from "./avatar";

type WearablePreviewProps = AvatarSceneOptions & {
  emoteNonce?: number;
  className?: string;
  style?: CSSProperties;
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
  onStatus,
}: WearablePreviewProps) {
  const ref = useRef<HTMLDivElement | null>(null);
  const sceneRef = useRef<AvatarScene | null>(null);
  const [status, setStatus] = useState<AvatarStatus>("loading");

  // Camera props are applied to the live scene (setCamera) rather than keyed into
  // the scene-creating effect — a zoom/yaw nudge must not tear down and reload GLBs.
  const cameraRef = useRef({ zoom, yaw, pitch, fov, targetY });
  cameraRef.current = { zoom, yaw, pitch, fov, targetY };

  const key = JSON.stringify([
    profile, Array.isArray(urns) ? urns : urns ?? null, body, outfit ?? null, model, base,
    controls, pan, platform, spin, spinSpeed, background,
    emotes ?? null,
  ]);

  useEffect(() => {
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
        ro = new ResizeObserver(() => scene?.resize());
        ro.observe(node);
      })
      .catch((err) => {
        if (!cancelled) {
          console.error("[WearablePreview]", err);
          setStatus("error");
        }
      });

    return () => {
      cancelled = true;
      if (ro) ro.disconnect();
      if (scene) scene.dispose();
      sceneRef.current = null;
    };
  }, [key]);

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
