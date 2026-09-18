import { useEffect, useRef, useState, type ComponentPropsWithoutRef, type ReactNode } from "react";
import "./coverimage.css";

export type CoverImageProps = Omit<ComponentPropsWithoutRef<"img">, "src" | "alt" | "onError" | "onLoad"> & {
  src?: string;
  alt: string;
  fallback?: ReactNode;
  fallbackSrc?: string;
};

export default function CoverImage({ src, alt, fallback, fallbackSrc, style, className = "", loading = "lazy", ...rest }: CoverImageProps) {
  const image = useRef<HTMLImageElement>(null);
  const [result, setResult] = useState<{ source?: string; loaded?: string; failed: string[] }>({ failed: [] });
  const current = result.source === src ? result : { source: src, failed: [] };
  const actual = src && current.failed.includes(src) && fallbackSrc ? fallbackSrc : src;
  const state = !actual ? "empty" : current.failed.includes(actual) ? "failed" : current.loaded === actual ? "ready" : "loading";
  useEffect(() => {
    if (image.current?.complete && image.current.naturalWidth > 0) setResult(old => ({ source: src, loaded: actual, failed: old.source === src ? old.failed : [] }));
  }, [actual, src]);
  return <span className={`ui-cover ${className}`} style={style} data-state={state}>
    {actual && state !== "failed" && <img {...rest} ref={image} src={actual} alt={alt} loading={loading} decoding="async"
      onLoad={() => setResult({ ...current, source: src, loaded: actual })} onError={() => setResult({ ...current, source: src, failed: [...current.failed, actual] })} />}
    {(state === "failed" || state === "empty") && <span className="ui-cover__fallback" role={alt ? "img" : undefined} aria-label={alt || undefined} aria-hidden={alt ? undefined : true}>
      {fallback ?? <svg viewBox="0 0 24 24" width="28" height="28" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><rect x="3" y="3" width="18" height="18" rx="3" /><circle cx="8" cy="8" r="1.5" /><path d="m3 17 5-5 4 4 4-6 5 7" /></svg>}
    </span>}
  </span>;
}
