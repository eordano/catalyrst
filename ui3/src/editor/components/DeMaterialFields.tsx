import { materialComponentValue, newTexture, object, textureValue, withMaterialComponent, type MaterialKind, type MaterialValue, type TextureKind } from "../gltf-materials";
import type { MaterialEdit } from "../material-selection";
import { hexToRgb, rgbToHex } from "./DeInspectorFields";

function NumberField({ label, value, fallback = 0, onChange, min, max }: { label: string; value: unknown; fallback?: number; onChange(value: number): void; min?: number; max?: number }) {
  return <label className="eui-prop"><span className="plabel">{label}</span><input className="eui-num" aria-label={label} type="number" step="any" min={min} max={max} value={typeof value === "number" && Number.isFinite(value) ? value : fallback} onChange={event => onChange(event.target.valueAsNumber)} /></label>;
}
function SelectField({ label, value, options, onChange }: { label: string; value: string; options: [string, string][]; onChange(value: string): void }) {
  return <label className="eui-prop"><span className="plabel">{label}</span><select className="eui-select" aria-label={label} value={value} onChange={event => onChange(event.target.value)}>{options.map(([key, text]) => <option key={key} value={key}>{text}</option>)}</select></label>;
}

function TextureFields({ label, value, assets, onChange }: { label: string; value: unknown; assets: string[]; onChange(value: unknown, paths?: string[][]): void }) {
  const texture = textureValue(value);
  const patch = (fields: MaterialValue, paths = Object.keys(fields).map(key => [key])) => onChange({ ...object(value), tex: { $case: texture.kind, [texture.kind]: { ...texture.value, ...fields } } }, paths.map(path => ["tex", texture.kind, ...path]));
  return <details className="eui-group" open={Boolean(value)}><summary>{label}</summary>
    <SelectField label={`${label} source`} value={value ? texture.kind : "none"} options={[["none", "No texture"], ["texture", "Image"], ["avatarTexture", "Avatar"], ["videoTexture", "Video entity"]]} onChange={kind => onChange(kind === "none" ? undefined : { tex: { $case: kind, [kind]: newTexture(kind as TextureKind) } })} />
    {value ? <>
      {texture.kind === "texture" ? <>
        <label className="eui-prop"><span className="plabel">Path or URL</span><input className="eui-input" aria-label={`${label} path or URL`} value={String(texture.value.src ?? "")} onChange={event => patch({ src: event.target.value })} /></label>
        <SelectField label={`${label} project asset`} value={assets.includes(String(texture.value.src)) ? String(texture.value.src) : ""} options={[["", assets.length ? "Choose project texture" : "No project textures"], ...assets.map(path => [path, path] as [string, string])]} onChange={src => { if (src) patch({ src }); }} />
        {(["offset", "tiling"] as const).map(key => <div key={key} className="eui-group">{["x", "y"].map(axis => <NumberField key={axis} label={`${label} ${key} ${axis.toUpperCase()}`} value={object(texture.value[key])[axis]} fallback={key === "tiling" ? 1 : 0} onChange={number => patch({ [key]: { ...object(texture.value[key]), [axis]: number } }, [[key, axis]])} />)}</div>)}
      </> : texture.kind === "videoTexture" ? <NumberField label={`${label} video entity`} value={texture.value.videoPlayerEntity} min={0} onChange={videoPlayerEntity => patch({ videoPlayerEntity })} /> : <label className="eui-prop"><span className="plabel">Avatar user ID</span><input className="eui-input" aria-label={`${label} avatar user ID`} value={String(texture.value.userId ?? "")} onChange={event => patch({ userId: event.target.value })} /></label>}
      <SelectField label={`${label} wrap`} value={String(texture.value.wrapMode ?? 0)} options={[["0", "Repeat"], ["1", "Clamp"], ["2", "Mirror"]]} onChange={wrapMode => patch({ wrapMode: Number(wrapMode) })} />
      <SelectField label={`${label} filter`} value={String(texture.value.filterMode ?? 0)} options={[["0", "Point"], ["1", "Bilinear"], ["2", "Trilinear"]]} onChange={filterMode => patch({ filterMode: Number(filterMode) })} />
    </> : null}
  </details>;
}

export function DeMaterialFields({ value, assets, onChange, mixedType = false }: { value: MaterialValue; assets: string[]; onChange(value: MaterialValue, edit: MaterialEdit): void; mixedType?: boolean }) {
  const material = materialComponentValue(value);
  const patch = (fields: MaterialValue, paths = Object.keys(fields).map(key => [key])) => onChange(withMaterialComponent(value, material.kind, { ...material.value, ...fields }), { paths: paths.map(path => ["material", material.kind, ...path]) });
  const numeric: [string, string, number][] = material.kind === "pbr" ? [["metallic", "Metallic", 0.5], ["roughness", "Roughness", 0.5], ["specularIntensity", "Specular intensity", 1], ["directIntensity", "Direct intensity", 1], ["emissiveIntensity", "Emissive intensity", 0], ["alphaTest", "Alpha cutoff", 0.5]] : [["alphaTest", "Alpha cutoff", 0.5]];
  const colors = material.kind === "pbr" ? [["albedoColor", "Albedo color", true], ["emissiveColor", "Emissive color", false], ["reflectivityColor", "Reflectivity color", false]] as const : [["diffuseColor", "Diffuse color", true]] as const;
  return <>
    <SelectField label="Material type" value={mixedType ? "mixed" : material.kind} options={[...(mixedType ? [["mixed", "Mixed \u2014 choose a type"] as [string, string]] : []), ["pbr", "PBR"], ["unlit", "Unlit"]]} onChange={kind => onChange(withMaterialComponent(value, kind as MaterialKind, { texture: material.value.texture, alphaTexture: material.value.alphaTexture, castShadows: material.value.castShadows ?? true }), { paths: [["material"]], replaceType: true })} />
    <fieldset disabled={mixedType} style={{ border: 0, padding: 0, margin: 0, minWidth: 0 }}>
    <label className="eui-prop"><input type="checkbox" checked={material.value.castShadows !== false} onChange={event => patch({ castShadows: event.target.checked })} />Cast material shadows</label>
    {colors.map(([key, label, alpha]) => <div key={key}>
      <label className="eui-prop"><span className="plabel">{label}</span><input type="color" className="eui-color-swatch" aria-label={label} value={rgbToHex(material.value[key] ?? (key === "emissiveColor" ? { r: 0, g: 0, b: 0 } : { r: 1, g: 1, b: 1 }))} onChange={event => { const color = hexToRgb(event.target.value, Number(object(material.value[key]).a ?? 1)); patch({ [key]: alpha ? color : { r: color.r, g: color.g, b: color.b } }, [[key, "r"], [key, "g"], [key, "b"]]); }} /></label>
      {alpha && <NumberField label={`${label} alpha`} value={object(material.value[key]).a} fallback={1} min={0} max={1} onChange={a => patch({ [key]: { r: 1, g: 1, b: 1, ...object(material.value[key]), a } }, [[key, "a"]])} />}
    </div>)}
    {numeric.map(([key, label, fallback]) => <NumberField key={key} label={label} value={material.value[key]} fallback={fallback} onChange={value => patch({ [key]: value })} />)}
    {material.kind === "pbr" && <SelectField label="Transparency" value={String(material.value.transparencyMode ?? 4)} options={[["0", "Opaque"], ["1", "Alpha test"], ["2", "Alpha blend"], ["3", "Alpha test and blend"], ["4", "Auto"]]} onChange={value => patch({ transparencyMode: Number(value) })} />}
    <TextureFields label="Base texture" value={material.value.texture} assets={assets} onChange={(value, paths = [[]]) => patch({ texture: value }, paths.map(path => ["texture", ...path]))} />
    <TextureFields label="Alpha texture" value={material.value.alphaTexture} assets={assets} onChange={(value, paths = [[]]) => patch({ alphaTexture: value }, paths.map(path => ["alphaTexture", ...path]))} />
    {material.kind === "pbr" && <><TextureFields label="Normal texture" value={material.value.bumpTexture} assets={assets} onChange={(value, paths = [[]]) => patch({ bumpTexture: value }, paths.map(path => ["bumpTexture", ...path]))} /><TextureFields label="Emissive texture" value={material.value.emissiveTexture} assets={assets} onChange={(value, paths = [[]]) => patch({ emissiveTexture: value }, paths.map(path => ["emissiveTexture", ...path]))} /></>}
    </fieldset>
  </>;
}
