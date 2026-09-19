import { useEffect, useId, useState } from "react";
import Modal from "../../components/Modal";
import DeSpawnAreas from "./DeSpawnAreas";
import { parseSceneDocument, readSceneSettings, updateSceneSettings, type SceneSettingsDraft, type SceneSettingsFile } from "../scene-settings";
import "./descenesettings.css";

export default function DeSceneSettings({ load, onClose }: { load(): Promise<SceneSettingsFile>; onClose(): void }) {
  const id = useId();
  const [file, setFile] = useState<SceneSettingsFile | null>(null);
  const [draft, setDraft] = useState<SceneSettingsDraft | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(false);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let disposed = false;
    setFile(null); setDraft(null); setError(null); setSaved(false);
    void load().then(next => {
      const values = readSceneSettings(parseSceneDocument(next.content));
      if (!disposed) { setFile(next); setDraft(values); }
    }).catch(error => { if (!disposed) setError(error instanceof Error ? error.message : String(error)); });
    return () => { disposed = true; };
  }, [load, attempt]);
  const field = (key: keyof Omit<SceneSettingsDraft, "spawnAreas">, label: string, multiline = false) => <div className="descene-field">
    <label htmlFor={`${id}-${key}`}>{label}</label>
    {multiline ? <textarea id={`${id}-${key}`} value={draft![key]} onChange={event => change(key, event.target.value)} rows={key === "parcels" ? 3 : 2} /> : <input id={`${id}-${key}`} value={draft![key]} onChange={event => change(key, event.target.value)} />}
  </div>;
  function change(key: keyof Omit<SceneSettingsDraft, "spawnAreas">, value: string) { setDraft(current => current && { ...current, [key]: value }); setSaved(false); }
  const select = (key: keyof Omit<SceneSettingsDraft, "spawnAreas">, label: string, options: string[][]) => <div className="descene-field"><label htmlFor={`${id}-${key}`}>{label}</label><select id={`${id}-${key}`} value={draft![key]} onChange={event => change(key, event.target.value)}><option value="">Scene default</option>{options.map(([value, text]) => <option value={value} key={value}>{text}</option>)}</select></div>;
  const toggles = [["enabled", "Enabled"], ["disabled", "Disabled"]];
  async function save() {
    if (!file || !draft || busy) return;
    setBusy(true); setError(null);
    try {
      const content = JSON.stringify(updateSceneSettings(parseSceneDocument(file.content), draft), null, 2) + "\n";
      await file.save(content);
      setFile({ ...file, content }); setDraft(readSceneSettings(parseSceneDocument(content))); setSaved(true);
    } catch (error) { setError(error instanceof Error ? error.message : String(error)); }
    finally { setBusy(false); }
  }
  return <Modal width={560} ariaLabel="Scene settings" onClose={busy ? undefined : onClose} closeOnBackdrop={false} showClose={false}>
    <form className="descene" onSubmit={event => { event.preventDefault(); void save(); }}>
      <h2>Scene settings</h2>
      {file && <p>Changes save to {file.destination.toLowerCase()}.</p>}
      {!draft && !error && <p role="status">Loading scene settings&#x2026;</p>}
      {draft && <fieldset disabled={busy}>
        {field("title", "Scene name")}
        {field("description", "Description", true)}
        {field("thumbnail", "Thumbnail path")}
        {field("tags", "Categories and tags, separated by commas")}
        <details><summary>Parcels and spawn areas</summary>
          {field("base", "Base parcel")}
          {field("parcels", "Parcels, one per line", true)}
          <DeSpawnAreas areas={draft.spawnAreas} onChange={spawnAreas => { setDraft(current => current && { ...current, spawnAreas }); setSaved(false); }} />
        </details>
        <details><summary>Environment and restrictions</summary>
          {field("fixedTime", "Fixed sky time, seconds since midnight")}
          {select("transition", "Sky transition", [["0", "Forward"], ["1", "Backward"]])}
          {select("terrain", "Surrounding terrain", [["true", "Visible"], ["false", "Hidden"]])}
          {select("voice", "Voice chat", toggles)}
          {select("nearbyVoice", "Nearby voice chat", toggles)}
          {select("portables", "Portable experiences", [...toggles, ["hideUi", "Hide interface"]])}
          {select("rating", "Age rating", [["A", "Adult"]])}
        </details>
      </fieldset>}
      {error && <p role="alert">{error}</p>}
      {saved && <p role="status">Saved to {file?.destination.toLowerCase()}.</p>}
      <div className="descene-actions">
        {error && <button type="button" disabled={busy} onClick={() => setAttempt(value => value + 1)}>Reload settings</button>}
        <button type="button" disabled={busy} onClick={onClose}>Close</button>
        <button type="submit" disabled={!draft || busy}>{busy ? "Saving\u2026" : "Save settings"}</button>
      </div>
    </form>
  </Modal>;
}
