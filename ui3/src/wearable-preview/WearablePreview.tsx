import { useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { previewHost } from "./bevy-host";
import type {
  AvatarOutfitOptions,
  AvatarScene,
  AvatarSceneOptions,
  AvatarStatus,
} from "./avatar";

type WearablePreviewProps = AvatarSceneOptions & {
  emoteNonce?: number;
  className?: string;
  style?: CSSProperties;
  pauseOffscreen?: boolean;
};

export const PREVIEW_LOAD_TIMEOUT_MS = 45000;

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
  const [booted, setBooted] = useState<boolean>(
    () => !(pauseOffscreen && typeof IntersectionObserver !== "undefined"),
  );

  const cameraRef = useRef({ zoom, yaw, pitch, fov, targetY });
  cameraRef.current = { zoom, yaw, pitch, fov, targetY };
  const outfitRef = useRef<AvatarOutfitOptions>({ profile, urns, body, outfit });
  outfitRef.current = { profile, urns, body, outfit };
  const emoteRef = useRef(emote);
  emoteRef.current = emote;
  const onStatusRef = useRef(onStatus);
  onStatusRef.current = onStatus;

  const key = JSON.stringify([
    model, base, controls, pan, platform, spin, spinSpeed, background, emotes ?? null,
  ]);
  const outfitKey = JSON.stringify([
    profile, Array.isArray(urns) ? urns : urns ?? null, body, outfit ?? null,
  ]);

  useEffect(() => {
    if (!booted) return;
    const el = ref.current;
    if (!el) return;
    let scene: AvatarScene | null = null;
    let ro: ResizeObserver | null = null;
    let cancelled = false;
    let deadline: ReturnType<typeof setTimeout> | undefined;
    const pending = new AbortController();
    const release = () => {
      pending.abort();
      ro?.disconnect();
      scene?.dispose();
      scene = null;
      sceneRef.current = null;
    };
    const fail = (err: unknown) => {
      if (cancelled) return;
      cancelled = true;
      clearTimeout(deadline);
      console.error("[WearablePreview]", err);
      release();
      setStatus("error");
      onStatusRef.current?.("error");
    };
    const watchLoading = () => {
      clearTimeout(deadline);
      deadline = setTimeout(
        () => fail(new Error("Avatar preview loading timed out")),
        PREVIEW_LOAD_TIMEOUT_MS,
      );
    };
    watchLoading();
    const contextLost = (event: Event) => {
      event.preventDefault();
      fail(new Error("Avatar preview WebGL context lost"));
    };
    el.addEventListener("webglcontextlost", contextLost, true);
    setStatus("loading");
    onStatusRef.current?.("loading");

    previewHost(pending.signal)
      .then((host) => host && !model ? import("./bevy") : import("./avatar"))
      .then(({ createAvatarScene }) => {
        const node = ref.current;
        if (cancelled || !node) return;
        scene = createAvatarScene(node, {
          ...outfitRef.current,
          model,
          emote: emoteRef.current,
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
            if (s === "loading") watchLoading();
            else clearTimeout(deadline);
            setStatus(s);
            onStatusRef.current?.(s);
          },
        });
        sceneRef.current = scene;
        scene.setActive(visibleRef.current);
        ro = new ResizeObserver(() => scene?.resize());
        ro.observe(node);
      })
      .catch(fail);

    return () => {
      cancelled = true;
      clearTimeout(deadline);
      el.removeEventListener("webglcontextlost", contextLost, true);
      release();
    };
  }, [key, booted]);

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
    void sceneRef.current?.setOutfit(outfitRef.current);
  }, [outfitKey]);

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
