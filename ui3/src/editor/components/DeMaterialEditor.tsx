import { useEffect, useState, type ReactNode } from "react";
import type { ProjectAssets } from "../types";
import { materialComponentError, object, textureAssetPaths, type MaterialValue } from "../gltf-materials";
import { DeMaterialFields } from "./DeMaterialFields";

export function DeMaterialDraft<T>({ value, normalize, validate, label, assets, onApply, children }: { value: unknown; normalize(value: unknown): T; validate(value: T): string | null; label: string; assets?: ProjectAssets; onApply?: (value: T) => Promise<void>; children(value: T, change: (value: T) => void, assetPaths: string[]): ReactNode }) {
  const serialized = JSON.stringify(value ?? {});
  const [baseline, setBaseline] = useState(serialized);
  const [draft, setDraft] = useState(() => normalize(value));
  const [dirty, setDirty] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [assetPaths, setAssetPaths] = useState<string[]>([]);
  const [assetError, setAssetError] = useState<string | null>(null);
  const [assetAttempt, setAssetAttempt] = useState(0);
  useEffect(() => { if (!dirty && !pending) { setDraft(normalize(JSON.parse(serialized))); setBaseline(serialized); } }, [serialized, dirty, pending, normalize]);
  useEffect(() => {
    let active = true;
    setAssetPaths([]); setAssetError(null);
    void assets?.list().then(files => { if (active) setAssetPaths(textureAssetPaths(files)); }, cause => { if (active) setAssetError(String(cause)); });
    return () => { active = false; };
  }, [assets, assetAttempt]);
  const change = (next: T) => { setDraft(next); setDirty(true); setError(null); };
  const conflict = dirty && baseline !== serialized;
  const validation = validate(draft);
  const apply = async () => {
    if (!onApply || pending || conflict || validation) return;
    setPending(true); setError(null);
    try { await onApply(draft); setDirty(false); }
    catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); }
    finally { setPending(false); }
  };
  return <div className="eui-gltf-modifiers">
    {assetError && <div role="alert">Could not list project textures: {assetError} <button className="eui-btn" onClick={() => setAssetAttempt(attempt => attempt + 1)}>Retry textures</button></div>}
    <fieldset disabled={!onApply || pending} style={{ border: 0, padding: 0, margin: 0, minWidth: 0 }}>
      {children(draft, change, assetPaths)}
    </fieldset>
    {conflict && <p role="alert">The component changed in the scene. Discard these edits to load its current values.</p>}
    {(error || validation) && <p role="alert">{error ?? validation}</p>}
    <div className="eui-prop"><button className="eui-btn primary" disabled={!onApply || !dirty || pending || conflict || Boolean(validation)} onClick={() => void apply()}>{pending ? "Applying\u2026" : label}</button><button className="eui-btn" disabled={!dirty || pending} onClick={() => { setDirty(false); setError(null); setDraft(normalize(value)); setBaseline(serialized); }}>Discard changes</button></div>
  </div>;
}

function materialDraft(value: unknown): MaterialValue { return structuredClone(object(value)); }

export function DeMaterialEditor({ value, assets, onApply }: { value: unknown; assets?: ProjectAssets; onApply?: (value: MaterialValue) => Promise<void> }) {
  return <DeMaterialDraft value={value} normalize={materialDraft} validate={materialComponentError} label="Apply material" assets={assets} onApply={onApply}>
    {(draft, change, paths) => <DeMaterialFields value={draft} assets={paths} onChange={change} />}
  </DeMaterialDraft>;
}
