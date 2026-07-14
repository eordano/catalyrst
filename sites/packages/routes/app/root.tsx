import { useEffect } from "react";
import { href } from "@core/lib/router/routes";
import {
  isRouteErrorResponse,
  Link,
  Links,
  Meta,
  Outlet,
  redirect,
  Scripts,
  ScrollRestoration,
  useRouteLoaderData,
} from "react-router";

import "@ui/atoms/primitives.css";

import SignInModalHost from "@features/components/auth/SignInModalHost";
import ChromeAuthBridge from "@features/components/chrome/ChromeAuthBridge";
import SyncEngineBridge from "@features/components/creator-hub/SyncEngineBridge";
import ErrorPage from "@ui/components/ErrorPage";
import type { ShouldRevalidateFunctionArgs } from "react-router";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { isCommitteeMember } from "@data/lib/catalyst/creator-hub/committee-membership.server";
import type { Route } from "./+types/root";

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const host = (request.headers.get("x-forwarded-host") ?? request.headers.get("host") ?? "")
    .split(":")[0];

  const thirdwebClientId =
    (typeof process !== "undefined" && process.env.THIRDWEB_CLIENT_ID) || "";
  const telemetryUrl =
    (typeof process !== "undefined" &&
      (process.env.VITE_TELEMETRY_URL || process.env.TELEMETRY_URL)) ||
    "";
  const wallet = readWallet(request);
  const committee = wallet
    ? await isCommitteeMember(wallet, request.signal).catch(() => false)
    : false;
  return {
    thirdwebClientId,
    telemetryUrl,
    wallet,
    committee,
  };
}

// The root payload (wallet cookie + committee flag) changes only through auth
// form submissions, never through in-page search-param navigation.
export function shouldRevalidate({
  currentUrl,
  nextUrl,
  formMethod,
  defaultShouldRevalidate,
}: ShouldRevalidateFunctionArgs) {
  if (formMethod) {
    return defaultShouldRevalidate;
  }
  if (currentUrl.pathname === nextUrl.pathname) {
    return false;
  }
  return defaultShouldRevalidate;
}

export function meta(_args: Route.MetaArgs) {
  return [{ title: "Decentraland" }];
}

export function Layout({ children }: { children: React.ReactNode }) {
  const data = useRouteLoaderData("root") as
    | { thirdwebClientId?: string; telemetryUrl?: string }
    | undefined;
  const publicEnv = JSON.stringify({
    thirdwebClientId: data?.thirdwebClientId ?? "",
    TELEMETRY_URL: data?.telemetryUrl ?? "",
  }).replace(/</g, "\\u003c");

  return (
    <html lang="en">
      <head>
        <meta charSet="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <script
          // eslint-disable-next-line react/no-danger
          dangerouslySetInnerHTML={{
            __html: `window.__DCL_PUBLIC__=${publicEnv}`,
          }}
        />
        <link rel="icon" type="image/png" href="/assets/dcl-logo.png" />
        <Meta />
        <Links />
      </head>
      <body>
        {children}
        <ScrollRestoration />
        <Scripts />
      </body>
    </html>
  );
}

const CLIENT_ERROR_ENDPOINT = "/internal/client-error";
const CLIENT_ERROR_BUDGET = 10;
const CLIENT_ERROR_DEDUPE_MS = 60_000;

function installClientErrorReporter(): () => void {
  if (typeof window === "undefined") return () => {};

  const seen = new Map<string, number>();
  let sent = 0;
  let reporting = false;

  const report = (name: string, message: string, stack: string) => {
    if (reporting || sent >= CLIENT_ERROR_BUDGET) return;
    const now = Date.now();
    const key = `${name}:${message}`;
    const last = seen.get(key);
    if (last !== undefined && now - last < CLIENT_ERROR_DEDUPE_MS) return;
    reporting = true;
    try {
      seen.set(key, now);
      sent += 1;
      void fetch(CLIENT_ERROR_ENDPOINT, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          message,
          name,
          stack,
          url: window.location.href,
          ua: navigator.userAgent,
          ts: new Date(now).toISOString(),
        }),
        keepalive: true,
      }).catch(() => {});
    } catch {
    } finally {
      reporting = false;
    }
  };

  const describe = (value: unknown): string => {
    if (typeof value === "string") return value;
    try {
      return JSON.stringify(value) ?? String(value);
    } catch {
      return String(value);
    }
  };

  const onError = (event: ErrorEvent) => {
    const err = event.error;
    if (err instanceof Error) {
      report(err.name || "Error", err.message || event.message, err.stack ?? "");
      return;
    }
    if (!event.message) return;
    report(
      "Error",
      event.message,
      `${event.filename ?? ""}:${event.lineno ?? 0}:${event.colno ?? 0}`,
    );
  };

  const onRejection = (event: PromiseRejectionEvent) => {
    const reason = event.reason;
    if (reason instanceof Error) {
      report(
        reason.name || "UnhandledRejection",
        reason.message || "(no message)",
        reason.stack ?? "",
      );
      return;
    }
    report("UnhandledRejection", describe(reason) || "(no message)", "");
  };

  const onResourceError = (event: Event) => {
    if (event instanceof ErrorEvent) return;
    const t = event.target;
    if (!(t instanceof Element)) return;
    const el = t as HTMLImageElement & HTMLScriptElement & HTMLLinkElement;
    const src = el.currentSrc || el.src || el.href || "";
    if (!src) return;
    report("ResourceError", `${t.tagName.toLowerCase()} failed to load: ${src}`.slice(0, 500), "");
  };

  const origError = console.error.bind(console);
  const origWarn = console.warn.bind(console);
  console.error = (...args: unknown[]) => {
    origError(...args);
    report("ConsoleError", args.map(describe).join(" ").slice(0, 500), "");
  };
  console.warn = (...args: unknown[]) => {
    origWarn(...args);
    report("ConsoleWarning", args.map(describe).join(" ").slice(0, 500), "");
  };

  window.addEventListener("error", onError);
  window.addEventListener("unhandledrejection", onRejection);
  window.addEventListener("error", onResourceError, true);
  return () => {
    window.removeEventListener("error", onError);
    window.removeEventListener("unhandledrejection", onRejection);
    window.removeEventListener("error", onResourceError, true);
    console.error = origError;
    console.warn = origWarn;
  };
}

let appHydrated = false;

export default function App() {
  useEffect(() => {
    appHydrated = true;
    if (import.meta.env.DEV) {
      import("@features/lib/dev/dev-kit").then((m) => m.installDevKit());
    }
    return installClientErrorReporter();
  }, []);
  return (
    <ChromeAuthBridge>
      <Outlet />
      <SignInModalHost />
      <SyncEngineBridge />
    </ChromeAuthBridge>
  );
}

export function ErrorBoundary({ error }: Route.ErrorBoundaryProps) {
  const isRouteError = isRouteErrorResponse(error);
  const routeStatus = isRouteError ? error.status : 0;

  let name = "Error";
  let message = "An unexpected error occurred.";
  let stack = "";
  if (error instanceof Error) {
    name = error.name || "Error";
    message = error.message || message;
    stack = error.stack || "";
  } else if (typeof error === "string") {
    message = error;
  } else if (error) {
    try {
      message = JSON.stringify(error);
    } catch {
      message = String(error);
    }
  }

  useEffect(() => {
    if (isRouteError) {
      if (routeStatus === 404 && appHydrated) {
        try {
          void fetch("/internal/page-not-found", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
              path: window.location.pathname + window.location.search,
              referrer: document.referrer,
            }),
            keepalive: true,
          }).catch(() => {});
        } catch {
        }
      }
      return;
    }
    try {
      void fetch(CLIENT_ERROR_ENDPOINT, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          message,
          name,
          stack,
          url: window.location.href,
          ua: navigator.userAgent,
          ts: new Date().toISOString(),
        }),
        keepalive: true,
      }).catch(() => {});
    } catch {
    }
  }, [isRouteError, routeStatus, name, message, stack]);

  if (isRouteError) {
    return <RouteErrorView error={error} />;
  }

  const detail = stack.trim().length > 0 ? stack : `${name}: ${message}`;

  return (
    <ErrorPage
      title="Oops!"
      message={message}
      detail={detail}
      isDev={import.meta.env.DEV}
    />
  );
}

function RouteErrorView({
  error,
}: {
  error: { status: number; statusText?: string; data?: string };
}) {
  const notFound = error.status === 404;
  const heading = notFound ? "404" : `Error ${error.status}`;
  const details = notFound
    ? "The requested page could not be found."
    : error.statusText || "Something went wrong loading this page.";
  const linkStyle: React.CSSProperties = {
    display: "inline-block",
    padding: "0.625rem 1.25rem",
    borderRadius: "0.5rem",
    fontWeight: 600,
    textDecoration: "none",
  };
  return (
    <main
      style={{
        minHeight: "100vh",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: "0.75rem",
        padding: "2rem",
        textAlign: "center",
        fontFamily: "Inter, system-ui, sans-serif",
        background: "#0d0c11",
        color: "#fff",
      }}
    >
      <h1 style={{ fontSize: "4rem", margin: 0 }}>{heading}</h1>
      <p style={{ margin: 0, opacity: 0.8 }}>{details}</p>
      <nav
        aria-label="Error page navigation"
        style={{
          display: "flex",
          gap: "0.75rem",
          marginTop: "1.25rem",
          flexWrap: "wrap",
          justifyContent: "center",
        }}
      >
        <Link
          to={href("/")}
          style={{ ...linkStyle, background: "var(--brand-cta)", color: "#fff" }}
        >
          Go to homepage
        </Link>
        <Link
          to={href("/create")}
          style={{
            ...linkStyle,
            background: "transparent",
            color: "#fff",
            border: "1px solid rgba(255,255,255,0.4)",
          }}
        >
          Open Creator Hub
        </Link>
      </nav>
    </main>
  );
}
