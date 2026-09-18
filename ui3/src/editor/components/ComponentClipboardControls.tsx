import { useState } from "react";

export interface ComponentClipboard {
  copy: (entity: string | number, name: string) => Promise<unknown>;
  paste: (name: string, payload: unknown) => Promise<unknown>;
}

let sessionClipboard: unknown;

export default function ComponentClipboardControls({ entity, name, actions }: {
  entity: string | number; name: string; actions: ComponentClipboard;
}) {
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const run = async (action: "copy" | "paste") => {
    if (busy) return;
    setBusy(true);
    setMessage(null);
    setFailed(false);
    try {
      if (action === "copy") {
        sessionClipboard = await actions.copy(entity, name);
        let system = false;
        try { await navigator.clipboard.writeText(JSON.stringify(sessionClipboard)); system = true; } catch {}
        setMessage(system ? "Component copied." : "Component copied within this editor.");
      } else {
        let text: string | undefined;
        try { text = await navigator.clipboard.readText(); } catch {}
        const payload = text ? JSON.parse(text) : sessionClipboard;
        const expected = name.includes("::") ? name : `core::${name}`;
        if (!payload || typeof payload !== "object" || payload.__dclComponent !== expected || !payload.value || typeof payload.value !== "object") {
          throw new Error(`Copy a ${name} component before pasting here.`);
        }
        await actions.paste(name, payload);
        setMessage("Component pasted to the selection.");
      }
    } catch (error) {
      setFailed(true);
      setMessage(error instanceof Error ? error.message : String(error));
    } finally { setBusy(false); }
  };
  return <span className="eui-component-clipboard" onClick={(event) => event.stopPropagation()}>
    <button className="eui-link" disabled={busy} aria-label={`Copy ${name} component`} onClick={() => void run("copy")}>Copy</button>
    <button className="eui-link" disabled={busy} aria-label={`Paste ${name} component to selection`} onClick={() => void run("paste")}>Paste</button>
    {message && <span role={failed ? "alert" : "status"} className="eui-comp-note">{message}</span>}
  </span>;
}
