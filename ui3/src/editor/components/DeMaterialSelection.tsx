import { changeSelection, selectionChanges, selectionDraft, selectionError, selectionHasMixedTypes, type AuthorMaterialSelection, type MaterialSelection } from "../material-selection";
import type { ProjectAssets } from "../types";
import { DeMaterialFields } from "./DeMaterialFields";
import { DeMaterialDraft } from "./DeMaterialEditor";

export function DeMaterialSelection({ selection, assets, onApply }: { selection: MaterialSelection[]; assets?: ProjectAssets; onApply?: AuthorMaterialSelection }) {
  const available = selection.every(item => item.value);
  const mixed = selection.some(item => JSON.stringify(item.value) !== JSON.stringify(selection[0]?.value));
  return <DeMaterialDraft value={selection} normalize={selectionDraft} validate={selectionError} label={`Apply to ${selection.length} materials`} assets={assets} onApply={onApply && available ? draft => onApply(selectionChanges(draft)) : undefined}>
    {(draft, change, paths) => <>
      <p className="eui-comp-note">Editing {selection.length} selected materials. {mixed ? "Values differ; fields show the active entity. " : ""}Only fields you change apply to the selection. Choosing a material type replaces its material settings.</p>
      <DeMaterialFields value={draft.value} assets={paths} mixedType={selectionHasMixedTypes(draft)} onChange={(value, edit) => change(changeSelection(draft, value, edit))} />
    </>}
  </DeMaterialDraft>;
}
