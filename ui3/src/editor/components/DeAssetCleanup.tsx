import { useRef, useState } from "react";
import type { DeWorkspaceCode } from "../types";
import { findUnreferencedAssets, type AssetCandidate } from "../project-assets";

export function DeAssetCleanup({ project, exportComposite }: { project: NonNullable<DeWorkspaceCode["project"]>; exportComposite?: () => Promise<string> }) {
  const [items, setItems] = useState<AssetCandidate[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const pending = useRef(false);
  const revisions = useRef(new Map<string, string>());
  const run = async (action: () => Promise<void>) => {
    if (pending.current) return;
    pending.current = true; setBusy(true); setError(null); setStatus(null);
    try { await action(); } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { pending.current = false; setBusy(false); }
  };
  const scan = async () => {
    if (!project.assets || !exportComposite) throw new Error("Stop the preview and connect the project before reviewing files.");
    return findUnreferencedAssets({ ...project, assets: project.assets }, await exportComposite());
  };
  if (!project.assets) return null;
  return <section className="eui-search" aria-label="Project asset cleanup" aria-busy={busy}>
    <button className="eui-btn" disabled={busy || !exportComposite} onClick={() => void run(async () => { const candidates = await scan();
      const versions = await Promise.all(candidates.map(async item => [item.path, project.assets!.revision ? await project.assets!.revision(item.path) : (await project.assets!.read(item.path)).revision] as const));
      revisions.current = new Map(versions);
      setItems(candidates); setSelected(new Set()); })}>Review unused files</button>
    {error && <p role="alert" className="eui-comp-note">{error}</p>}
    {status && <p role="status" className="eui-comp-note">{status}</p>}
    {items && <>
      <p className="eui-comp-note">{items.length ? "No static reference was found for these files in the scene, custom items, scripts, or model dependencies. Select files to remove from this project." : "Every asset has a project reference."}</p>
      {items.map(item => <label key={item.path} className="eui-row" title={item.path}>
        <input type="checkbox" checked={selected.has(item.path)} disabled={busy} onChange={event => setSelected(previous => { const next = new Set(previous); if (event.target.checked) next.add(item.path); else next.delete(item.path); return next; })} />
        <span className="label">{item.path}</span><span className="dim">{Math.ceil(item.size / 1024)} KB</span>
      </label>)}
      {!!items.length && <button className="eui-btn" disabled={busy || !selected.size} onClick={() => void run(async () => {
        const current = new Set((await scan()).map(item => item.path));
        for (const path of selected) if (!current.has(path)) throw new Error(`${path} is now referenced. Review the files again before removing it.`);
        let removed = 0;
        try {
          for (const path of selected) {
            const revision = revisions.current.get(path);
            if (!revision) throw new Error(`Review ${path} again before removing it.`);
            await project.assets!.remove(path, revision);
            removed++;
            setItems(previous => previous?.filter(item => item.path !== path) ?? null);
            setSelected(previous => new Set([...previous].filter(item => item !== path)));
          }
        } finally { if (removed) setStatus(`Removed ${removed} file${removed === 1 ? "" : "s"} from this project.`); }
      })}>Remove selected files ({selected.size})</button>}
    </>}
  </section>;
}
