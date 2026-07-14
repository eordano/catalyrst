import { useState } from "react";

import { SIDECAR_START_COMMAND } from "./rules";

import "./mpunpaired.css";

export type MpUnpairedProps = {
  reason: "no-pairing" | "unreachable";
  port?: number | null;
  desktopShellOnHttps?: boolean;
  devOriginUrl?: string;
};

export default function MpUnpaired({
  reason,
  port = null,
  desktopShellOnHttps = false,
  devOriginUrl,
}: MpUnpairedProps) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(SIDECAR_START_COMMAND);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
    }
  };

  return (
    <section className="mp-unpaired" aria-label="Multiplayer test sidecar setup">
      <h3 className="mp-unpaired__title">
        {reason === "unreachable"
          ? `Waiting for the mp-testd sidecar${port ? ` on 127.0.0.1:${port}` : ""}…`
          : "Pair the mp-testd sidecar to run multiplayer tests"}
      </h3>
      <p className="mp-unpaired__body">
        The panel drives external test clients through a local sidecar — it
        never hosts engines in this page. Start it from the repo root:
      </p>
      <div className="mp-unpaired__command" data-mp-command="">
        <code>{SIDECAR_START_COMMAND}</code>
        <button type="button" data-mp-action="copy-command" onClick={copy}>
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <p className="mp-unpaired__body">
        {reason === "unreachable"
          ? "Pairing is saved — this page reconnects automatically once the sidecar is up."
          : "The sidecar prints a pairing link (?mpd=<port>#mpdtoken=<token>) on startup — open it, and this page remembers the pairing."}
      </p>
      {desktopShellOnHttps && (
        <p className="mp-unpaired__shell" data-mp-note="shell-origin">
          You are in the desktop shell on an https origin. WebKitGTK&rsquo;s
          https&nbsp;→&nbsp;ws://127.0.0.1 mixed-content behavior is unverified,
          so pairing may silently fail here — use the http://localhost dev
          origin instead:{" "}
          {devOriginUrl ? (
            <a href={devOriginUrl} data-mp-action="dev-origin">
              {devOriginUrl}
            </a>
          ) : null}
        </p>
      )}
    </section>
  );
}
