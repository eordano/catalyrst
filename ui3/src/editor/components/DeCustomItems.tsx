import { useEffect, useRef, useState } from "react";
import type { DeWorkspaceCode } from "../types";
import { customItemPath, customItemPayload, customItems, customItemStore, type CustomItem, type EntityCopy } from "../custom-items";

export function DeCustomItems({ code, selectionName, onCapture, onPlace }: {
  code: DeWorkspaceCode;
  selectionName?: string | null;
  onCapture?: () => Promise<unknown>;
  onPlace?: (payload: EntityCopy) => Promise<void>;
}) {
  const [items, setItems] = useState<CustomItem[]>([]);
  const [name, setName] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  useEffect(() => {
    let alive = true;
    void customItemStore(code).then(store => store.list()).then(paths => { if (alive) setItems(customItems(paths)); }).catch(reason => { if (alive) setError(String(reason)); });
    return () => { alive = false; };
  }, [code]);
  const act = async (operation: () => Promise<void>) => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true); setError(null); setStatus(null);
    try { await operation(); } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { pending.current = false; setBusy(false); }
  };
  return <div className="eui-panel-body" aria-busy={busy}>
    <div className="eui-search">
      <label htmlFor="custom-item-name">Custom item name</label>
      <input id="custom-item-name" className="eui-input" placeholder={selectionName ?? "Custom item"} value={name} onChange={event => setName(event.target.value)} />
      <button className="eui-btn" disabled={!onCapture || busy} onClick={() => void act(async () => {
        if (!onCapture) return;
        const payload = await onCapture() as EntityCopy;
        customItemPayload(payload.composite);
        const store = await customItemStore(code);
        const path = customItemPath(name.trim() || selectionName || "Custom item", await store.list());
        await store.write(path, payload.composite);
        setItems(customItems(await store.list()));
        setStatus("Custom item saved in assets/custom-items.");
      })}>Save selection as custom item</button>
      <p className="eui-comp-note">Includes selected entities and their children. Model and media files stay in this project.</p>
    </div>
    {error && <div className="eui-empty" role="alert">{error}</div>}
    {status && <div className="eui-comp-note" role="status">{status}</div>}
    {!items.length && <div className="eui-empty">Save a selection to reuse it in this project.</div>}
    {items.map(item => <div className="eui-row" key={item.path}>
      <span className="label" title={item.path}>{item.name}</span>
      <button className="eui-btn" disabled={!onPlace || busy} aria-label={`Place ${item.name}`} onClick={() => void act(async () => {
        const store = await customItemStore(code);
        const payload = customItemPayload(await store.read(item.path));
        await onPlace?.(payload);
        setStatus(`${item.name} placed in the scene.`);
      })}>Place</button>
    </div>)}
  </div>;
}
