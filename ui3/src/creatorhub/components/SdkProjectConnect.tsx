import { useId, useState } from "react";
import "./sdk-project-connect.css";

export default function SdkProjectConnect({ initialUrl = "", onOpen }: {
  initialUrl?: string; onOpen: (url: string) => Promise<void>;
}) {
  const id = useId();
  const [url, setUrl] = useState(initialUrl);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  return <form className="ch-sdk-project" onSubmit={(event) => {
    event.preventDefault();
    if (busy) return;
    setBusy(true);
    setError(null);
    void onOpen(url).catch((error: unknown) => setError(error instanceof Error ? error.message : String(error))).finally(() => setBusy(false));
  }}>
    <label htmlFor={id}>Open SDK project</label>
    <p id={`${id}-hint`}>Run <code>dcl-one-sdk start</code> in your project, then open the same files here.</p>
    <div className="ch-sdk-project__controls">
      <input id={id} type="url" required value={url} onChange={(event) => setUrl(event.target.value)}
        placeholder="http://localhost:8000" aria-describedby={`${id}-hint`} autoComplete="off" spellCheck={false} disabled={busy} />
      <button type="submit" disabled={busy}>{busy ? "Connecting\u2026" : "Open project"}</button>
    </div>
    {error && <p role="alert">{error}</p>}
  </form>;
}
