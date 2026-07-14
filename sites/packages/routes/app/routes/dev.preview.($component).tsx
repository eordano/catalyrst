
import { useEffect, useState, type CSSProperties } from "react";
import { useParams, useSearchParams } from "react-router";

import type { Route } from "./+types/dev.preview.($component)";

export function meta(_args: Route.MetaArgs) {
  return [{ title: "Component preview \u{2014} dev" }];
}

type Loaded =
  | { state: "loading" }
  | { state: "index"; entries: { key: string; path: string }[] }
  | { state: "ready"; Component: React.ComponentType<Record<string, unknown>>; path: string }
  | { state: "error"; message: string };

const wrapStyles: Record<string, CSSProperties> = {
  dark: { background: "#0d0c11", minHeight: "100vh", padding: 24 },
  light: { background: "#ffffff", minHeight: "100vh", padding: 24 },
};

export default function DevPreviewRoute() {
  const { component } = useParams();
  const [params] = useSearchParams();
  const [loaded, setLoaded] = useState<Loaded>({ state: "loading" });

  useEffect(() => {
    if (!import.meta.env.DEV) {
      setLoaded({ state: "error", message: "dev-server only" });
      return;
    }
    let live = true;
    (async () => {
      const reg = await import("@features/lib/dev/preview-registry");
      if (!component) {
        const entries = reg.listEntries().map(({ key, path }) => ({ key, path }));
        if (live) setLoaded({ state: "index", entries });
        return;
      }
      try {
        const resolved = await reg.resolveComponent(component);
        if (!live) return;
        if (!resolved) {
          setLoaded({ state: "error", message: `unknown component "${component}"` });
        } else {
          setLoaded({ state: "ready", ...resolved });
        }
      } catch (err) {
        if (live) {
          setLoaded({ state: "error", message: (err as Error)?.message ?? "load failed" });
        }
      }
    })();
    return () => {
      live = false;
    };
  }, [component]);

  if (!import.meta.env.DEV || loaded.state === "error") {
    const message = loaded.state === "error" ? loaded.message : "dev-server only";
    return (
      <div style={{ padding: 32, fontFamily: "monospace" }}>
        <h1>/dev/preview</h1>
        <p>{message}</p>
      </div>
    );
  }

  if (loaded.state === "loading") return null;

  if (loaded.state === "index") {
    return (
      <div style={{ padding: 32, fontFamily: "monospace", lineHeight: 1.9 }}>
        <h1>/dev/preview &#x2014; {loaded.entries.length} components</h1>
        <p>
          Usage: /dev/preview/&lt;key&gt;?props=&#123;"children":"Hi"&#125;
          &amp;wrap=ui2&amp;bg=dark
        </p>
        <ul>
          {loaded.entries.map((e) => (
            <li key={e.path}>
              <a href={`/dev/preview/${encodeURIComponent(e.key)}`}>{e.key}</a>
            </li>
          ))}
        </ul>
      </div>
    );
  }

  let props: Record<string, unknown> = {};
  const rawProps = params.get("props");
  if (rawProps) {
    try {
      props = JSON.parse(rawProps) as Record<string, unknown>;
    } catch {
      return (
        <div style={{ padding: 32, fontFamily: "monospace" }}>
          props is not valid JSON: {rawProps}
        </div>
      );
    }
  }

  const bg = params.get("bg") ?? "dark";
  const wrap = params.get("wrap");
  const { Component } = loaded;
  const body = (
    <PreviewErrorBoundary key={JSON.stringify(props)}>
      <Component {...props} />
    </PreviewErrorBoundary>
  );
  return (
    <div style={wrapStyles[bg] ?? wrapStyles.dark} data-preview-root>
      {wrap ? <div className={wrap}>{body}</div> : body}
    </div>
  );
}

import { Component as ReactComponent, type ReactNode } from "react";

class PreviewErrorBoundary extends ReactComponent<
  { children: ReactNode },
  { error: Error | null }
> {
  state = { error: null as Error | null };
  static getDerivedStateFromError(error: Error) {
    return { error };
  }
  render() {
    if (this.state.error) {
      return (
        <pre style={{ color: "#ff5c77", padding: 16 }}>
          render error: {String(this.state.error?.message ?? this.state.error)}
        </pre>
      );
    }
    return this.props.children;
  }
}
