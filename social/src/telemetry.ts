const recent = new Map<string, number>();
const scrub = (text: string) => text.replace(/0x[a-f0-9]{40,}|(?:bearer\s+)[^\s]+|(?:privatekey|signature|token|password)[\s":=]+[^\s,}]+|\?[^\s)]+/gi, "[redacted]");
export function reportError(error: unknown, kind = "client") {
  const message = scrub(error instanceof Error ? error.message : typeof error === "string" ? error : "Unexpected client error").slice(0, 2000);
  const now = Date.now();
  for (const [key, time] of recent) if (now - time > 60000) recent.delete(key);
  const key = `${kind}:${message}`;
  if (recent.has(key) || recent.size >= 20) return;
  recent.set(key, now);
  void fetch(new URL("api/telemetry", document.baseURI), {
    method: "POST", headers: { "Content-Type": "application/json" }, keepalive: true,
    body: JSON.stringify({ kind, message, stack: error instanceof Error ? scrub(error.stack || "").slice(0, 4000) : "" }),
  }).catch(() => {});
}
export function installTelemetry() {
  window.addEventListener("securitypolicyviolation", event => {
    reportError(`Blocked ${event.effectiveDirective}: ${event.blockedURI}`, "csp");
  });
  window.addEventListener("error", event => { if (event instanceof ErrorEvent) reportError(event.error || event.message, "uncaught"); });
  window.addEventListener("unhandledrejection", event => reportError(event.reason, "unhandledrejection"));
  const original = console.error.bind(console);
  console.error = (...args: unknown[]) => {
    original(...args);
    reportError(args.find(arg => arg instanceof Error) || args.filter(arg => typeof arg === "string").join(" ") || "Console error", "console.error");
  };
}
