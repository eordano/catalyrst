export function publicTelemetryUrl(configured: string | undefined): string {
  if (!configured) return "";
  try {
    const host = new URL(configured).hostname;
    if (host === "localhost" || host === "[::1]" || host === "0.0.0.0" || /^127\./.test(host)) return "/telemetry";
  } catch {
    return configured.startsWith("/") && !configured.startsWith("//") ? configured : "";
  }
  return configured;
}
