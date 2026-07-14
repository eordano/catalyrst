import type { RefObject } from "react";

export default function EngineViewport({
  viewportRef,
  src,
  onLoad,
}: {
  viewportRef: RefObject<HTMLIFrameElement | null> | null;
  src: string;
  onLoad: () => void;
}) {
  return (
    <iframe
      ref={viewportRef}
      className="eui-vp-frame"
      src={src}
      title="Scene viewport"
      allow="cross-origin-isolated; autoplay; fullscreen; clipboard-read; clipboard-write; xr-spatial-tracking; gamepad; microphone; camera"
      onLoad={onLoad}
    />
  );
}
