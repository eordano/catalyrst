export type PreviewCommand = Record<string, unknown> & { op: string };
export interface BevyPreviewSession {
  send(command: PreviewCommand): void;
  dispose(): void;
}
export interface BevyPreviewHost {
  create(onFrame: (bitmap: ImageBitmap, generation: number) => void): Promise<BevyPreviewSession>;
}
declare global {
  interface Window { dclAvatarPreview?: BevyPreviewHost; dclAvatarPreviewExpected?: boolean }
}
export async function previewHost(signal: AbortSignal): Promise<BevyPreviewHost | undefined> {
  if (window.dclAvatarPreviewExpected && !window.dclAvatarPreview) {
    await new Promise<void>((resolve, reject) => {
      const cleanup = () => {
        window.removeEventListener("dcl-avatar-preview-ready", ready);
        signal.removeEventListener("abort", abort);
      };
      const ready = () => { cleanup(); resolve(); };
      const abort = () => { cleanup(); reject(new DOMException("Preview disposed", "AbortError")); };
      window.addEventListener("dcl-avatar-preview-ready", ready, { once: true });
      signal.addEventListener("abort", abort, { once: true });
      if (signal.aborted) abort();
    });
  }
  return window.dclAvatarPreview;
}
