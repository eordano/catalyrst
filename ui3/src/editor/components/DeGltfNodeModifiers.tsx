import { useId } from "react";
import type { ProjectAssets } from "../types";
import { materialSwapError, modifiersValue, newMaterialSwap, object, type GltfModifiers, type GltfSwap } from "../gltf-materials";
import { DeMaterialFields } from "./DeMaterialFields";
import { DeMaterialDraft } from "./DeMaterialEditor";

function SwapFields({ swap, index, paths, assets, onChange, onRemove }: { swap: GltfSwap; index: number; paths: string[]; assets: string[]; onChange(value: GltfSwap): void; onRemove(): void }) {
  const uid = useId();
  return <details className="eui-group" open><summary>Swap {index + 1}{swap.path ? ` \u00b7 ${swap.path}` : " \u00b7 Whole model"}</summary>
    <label className="eui-prop"><span className="plabel">Node path</span><input className="eui-input" aria-label={`Swap ${index + 1} node path`} list={uid} placeholder="Whole model" value={swap.path ?? ""} onChange={event => onChange({ ...swap, path: event.target.value })} /><datalist id={uid}>{paths.map(path => <option key={path} value={path} />)}</datalist></label>
    <p className="eui-comp-note">Leave the path empty to affect the whole model.</p>
    <label className="eui-prop"><input type="checkbox" checked={swap.castShadows ?? true} onChange={event => onChange({ ...swap, castShadows: event.target.checked })} />Cast node shadows</label>
    <DeMaterialFields value={object(swap.material)} assets={assets} onChange={material => onChange({ ...swap, material })} />
    <button className="eui-btn" type="button" onClick={onRemove}>Remove swap {index + 1}</button>
  </details>;
}

export function DeGltfNodeModifiers({ value, nodePaths = [], assets, onApply }: { value: unknown; nodePaths?: string[]; assets?: ProjectAssets; onApply?: (value: GltfModifiers) => Promise<void> }) {
  return <DeMaterialDraft value={value} normalize={modifiersValue} validate={materialSwapError} label="Apply material swaps" assets={assets} onApply={onApply}>
    {(draft, change, paths) => <>
      <p className="eui-comp-note">Swap model materials or override individual nodes.</p>
      {draft.modifiers.length === 0 && <p className="eui-comp-note">The model uses its original materials.</p>}
      {draft.modifiers.map((swap, index) => <SwapFields key={index} swap={swap} index={index} paths={nodePaths} assets={paths} onChange={next => change({ ...draft, modifiers: draft.modifiers.map((item, i) => i === index ? next : item) })} onRemove={() => change({ ...draft, modifiers: draft.modifiers.filter((_, i) => i !== index) })} />)}
      <button className="eui-btn" onClick={() => change({ ...draft, modifiers: [...draft.modifiers, newMaterialSwap()] })}>Add material swap</button>
    </>}
  </DeMaterialDraft>;
}
