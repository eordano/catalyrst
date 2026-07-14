import { useEffect, useRef, useState } from "react";

export type ErrorPageProps = {
  title: string;
  message: string;
  detail: string;
  isDev: boolean;
};

const REPORT_ENDPOINT = "/internal/client-error";

export function truncateOneLine(input: string, max = 200): string {
  const flat = (input ?? "").replace(/\s+/g, " ").trim();
  if (!flat) return "An unexpected error occurred.";
  return flat.length > max ? `${flat.slice(0, max - 1)}\u{2026}` : flat;
}

export function parseErrorName(detail: string): string {
  const first = ((detail ?? "").split("\n")[0] || "").trim();
  const m = first.match(/^([A-Za-z_$][\w$]*(?:Error|Exception))\b/);
  return m?.[1] ?? "Error";
}

export function composeTrace(detail: string, url: string, ts: string): string {
  const body = (detail ?? "").trim() || "No further detail was captured.";
  return `${body}\n\nURL:  ${url || "(unknown)"}\nTime: ${ts}`;
}

function reportOnce(detail: string, message: string, url: string, ts: string): void {
  try {
    const payload = JSON.stringify({
      message: truncateOneLine(message, 500),
      name: parseErrorName(detail),
      stack: (detail ?? "").slice(0, 8000),
      url,
      ua: typeof navigator !== "undefined" ? navigator.userAgent : "",
      ts,
    });
    const canBeacon =
      typeof navigator !== "undefined" && typeof navigator.sendBeacon === "function";
    if (canBeacon) {
      const blob = new Blob([payload], { type: "application/json" });
      if (navigator.sendBeacon(REPORT_ENDPOINT, blob)) return;
    }
    void fetch(REPORT_ENDPOINT, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: payload,
      keepalive: true,
    }).catch(() => {});
  } catch {
  }
}

export default function ErrorPage({ title, message, detail, isDev }: ErrorPageProps) {
  const restartRef = useRef<HTMLButtonElement | null>(null);
  const traceRef = useRef<HTMLTextAreaElement | null>(null);
  const sentRef = useRef(false);
  const [copied, setCopied] = useState(false);
  const [trace, setTrace] = useState(() => composeTrace(detail, "", "\u{2026}"));

  useEffect(() => {
    const url = typeof window !== "undefined" ? window.location.href : "";
    const ts = new Date().toISOString();
    setTrace(composeTrace(detail, url, ts));

    restartRef.current?.focus();

    if (!sentRef.current) {
      sentRef.current = true;
      reportOnce(detail, message, url, ts);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function copyTrace(): Promise<void> {
    const text = traceRef.current?.value ?? trace;
    let ok = false;
    try {
      if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(text);
        ok = true;
      }
    } catch {
      ok = false;
    }
    if (!ok) {
      try {
        const ta = traceRef.current;
        if (ta) {
          ta.focus();
          ta.select();
          ok = document.execCommand("copy");
          ta.setSelectionRange(text.length, text.length);
          restartRef.current?.focus();
        }
      } catch {
        ok = false;
      }
    }
    if (ok) {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    }
  }

  function restart(): void {
    try {
      window.location.assign("/");
    } catch {
      window.location.reload();
    }
  }

  const oneLine = truncateOneLine(message);

  return (
    <>
      <style dangerouslySetInnerHTML={{ __html: STYLE }} />
      <main className="error-page" role="alert" aria-labelledby="ep-title">
        <div className="ep-card">
          <span className="ep-chip">
            <span className="dot" aria-hidden="true" />
            System offline
            {isDev ? <span className="ep-dev">dev</span> : null}
          </span>

          <h1 id="ep-title" className="oops" data-text={title}>
            {title}
          </h1>

          <div className="ep-block">
            <p className="ep-label">An unexpected error happened:</p>
            <p className="ep-oneline" title={message}>
              {oneLine}
            </p>
          </div>

          <div className="ep-block">
            <label className="ep-label" htmlFor="ep-trace">
              Here is more info if you were working on this:
            </label>
            <textarea
              id="ep-trace"
              ref={traceRef}
              className="ep-trace"
              readOnly
              spellCheck={false}
              value={trace}
              onChange={() => {}}
              aria-label="Full error trace"
            />
          </div>

          <p className="ep-notified">
            <span className="dot" aria-hidden="true" />
            Our developers have been notified of this issue!
          </p>

          <p className="ep-playful">Have you tried turning it off and on again?</p>

          <div className="ep-actions">
            <button
              type="button"
              className="ep-btn ep-btn-secondary"
              onClick={copyTrace}
            >
              {copied ? "Copied!" : "Copy Error trace"}
            </button>
            <button
              type="button"
              ref={restartRef}
              className="ep-btn ep-btn-primary"
              onClick={restart}
            >
              Restart
            </button>
          </div>
          <span className="ep-copied" role="status" aria-live="polite">
            {copied ? "Error trace copied to clipboard" : ""}
          </span>
        </div>
      </main>
    </>
  );
}

const STYLE = `
.error-page {
  min-height: 100vh;
  box-sizing: border-box;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: clamp(1.25rem, 4vw, 3rem);
  background:
    radial-gradient(90% 60% at 50% -10%, rgba(255,45,85,0.16), transparent 62%),
    radial-gradient(70% 50% at 100% 110%, rgba(34,211,238,0.08), transparent 60%),
    #0d0c11;
  color: #f4f2f7;
  font-family: Inter, system-ui, -apple-system, "Segoe UI", sans-serif;
  -webkit-font-smoothing: antialiased;
}
.error-page * { box-sizing: border-box; }
.error-page :focus-visible {
  outline: 2px solid #ff2d55;
  outline-offset: 2px;
  border-radius: 4px;
}
.error-page .ep-card {
  width: 100%;
  max-width: 40rem;
  display: flex;
  flex-direction: column;
  gap: 1.05rem;
  text-align: left;
}
.error-page .ep-chip {
  align-self: flex-start;
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 0.72rem;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: #ff8fa3;
  background: rgba(255,45,85,0.10);
  border: 1px solid rgba(255,45,85,0.24);
  padding: 0.35rem 0.65rem;
  border-radius: 999px;
}
.error-page .ep-chip .dot {
  width: 0.5rem; height: 0.5rem; border-radius: 50%;
  background: #ff2d55;
  animation: ep-pulse 2s infinite;
}
.error-page .ep-dev {
  color: #cbc7d6;
  background: rgba(255,255,255,0.08);
  border-radius: 999px;
  padding: 0.05rem 0.45rem;
  letter-spacing: 0.1em;
}
.error-page .oops {
  position: relative;
  margin: 0.15rem 0 0.1rem;
  font-size: clamp(3.5rem, 12vw, 6rem);
  font-weight: 800;
  letter-spacing: -0.03em;
  line-height: 0.95;
  color: #ffffff;
}
.error-page .oops::before,
.error-page .oops::after {
  content: attr(data-text);
  position: absolute;
  left: 0; top: 0;
  width: 100%;
  pointer-events: none;
  opacity: 0;
}
.error-page .oops::before { color: #ff2d55; animation: ep-glitch-a 3.4s infinite steps(2, end); }
.error-page .oops::after  { color: #22d3ee; animation: ep-glitch-b 2.7s infinite steps(2, end); }
.error-page .ep-block { display: flex; flex-direction: column; }
.error-page .ep-label {
  margin: 0 0 0.4rem;
  font-size: 0.82rem;
  color: #9a97a8;
}
.error-page .ep-oneline {
  margin: 0;
  max-width: 100%;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 0.88rem;
  color: #ff9caf;
  background: rgba(255,45,85,0.08);
  border: 1px solid rgba(255,45,85,0.20);
  border-radius: 0.5rem;
  padding: 0.55rem 0.8rem;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.error-page .ep-trace {
  width: 100%;
  min-height: 9.5rem;
  resize: vertical;
  padding: 0.8rem 0.9rem;
  background: #08070b;
  color: #cbc8d8;
  border: 1px solid rgba(255,255,255,0.10);
  border-radius: 0.6rem;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 0.78rem;
  line-height: 1.55;
  white-space: pre;
  overflow: auto;
  tab-size: 2;
}
.error-page .ep-trace:focus-visible { border-color: #ff2d55; }
.error-page .ep-notified {
  margin: 0.1rem 0 0;
  display: flex;
  align-items: center;
  gap: 0.55rem;
  font-size: 0.92rem;
  color: #8ee6ac;
}
.error-page .ep-notified .dot {
  width: 0.5rem; height: 0.5rem; border-radius: 50%;
  background: #43d787;
  animation: ep-pulse-green 2s infinite;
}
.error-page .ep-playful {
  margin: 0;
  font-style: italic;
  font-size: 0.9rem;
  color: #8c8998;
}
.error-page .ep-actions {
  display: flex;
  gap: 0.75rem;
  margin-top: 0.4rem;
}
.error-page .ep-btn {
  flex: 1 1 0;
  min-width: 0;
  padding: 0.85rem 1rem;
  border-radius: 0.65rem;
  font-family: inherit;
  font-size: 0.95rem;
  font-weight: var(--fw-semibold, 600);
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease, transform 0.05s ease;
}
.error-page .ep-btn:active { transform: translateY(1px); }
.error-page .ep-btn-secondary {
  background: transparent;
  color: #f4f2f7;
  border: 1px solid rgba(255,255,255,0.22);
}
.error-page .ep-btn-secondary:hover {
  background: rgba(255,255,255,0.05);
  border-color: rgba(255,255,255,0.42);
}
.error-page .ep-btn-primary {
  background: var(--brand-cta);
  color: #ffffff;
  border: 1px solid var(--brand);
  box-shadow: 0 8px 26px rgba(255,45,85,0.35);
}
.error-page .ep-btn-primary:hover { background: #ff4668; }
.error-page .ep-copied {
  min-height: 1rem;
  font-size: 0.78rem;
  color: #8ee6ac;
}
@keyframes ep-pulse {
  0%   { box-shadow: 0 0 0 0 rgba(255,45,85,0.55); }
  70%  { box-shadow: 0 0 0 8px rgba(255,45,85,0); }
  100% { box-shadow: 0 0 0 0 rgba(255,45,85,0); }
}
@keyframes ep-pulse-green {
  0%   { box-shadow: 0 0 0 0 rgba(67,215,135,0.5); }
  70%  { box-shadow: 0 0 0 7px rgba(67,215,135,0); }
  100% { box-shadow: 0 0 0 0 rgba(67,215,135,0); }
}
@keyframes ep-glitch-a {
  0%, 92%, 100% { transform: translate(0, 0); opacity: 0; }
  93% { transform: translate(-2px, 1px); opacity: 0.55; }
  96% { transform: translate(2px, -1px); opacity: 0.45; }
}
@keyframes ep-glitch-b {
  0%, 90%, 100% { transform: translate(0, 0); opacity: 0; }
  91% { transform: translate(2px, -1px); opacity: 0.5; }
  95% { transform: translate(-2px, 1px); opacity: 0.4; }
}
@media (prefers-reduced-motion: reduce) {
  .error-page .oops::before,
  .error-page .oops::after { animation: none; opacity: 0; }
  .error-page .ep-chip .dot,
  .error-page .ep-notified .dot { animation: none; }
}
`;
