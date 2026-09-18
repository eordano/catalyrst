import { useEffect, useRef, useState } from "react";
import { execute, type Scene, type WalletIdentity } from "./api";
import { Dialog } from "./Dialog";
import { ScenePicker } from "./ScenePicker";
import { destinationLabel } from "./destinations";

export function EventEditor({ identity, onConnect, onClose }: {
  identity: WalletIdentity | null; onConnect: () => void; onClose: () => void;
}) {
  const [name, setName] = useState(""), [description, setDescription] = useState("");
  const [start, setStart] = useState(""), [end, setEnd] = useState("");
  const [scene, setScene] = useState<Scene | null>(null), [picker, setPicker] = useState(false);
  const [busy, setBusy] = useState(false), [error, setError] = useState("");
  const [created, setCreated] = useState(false);
  const generation = useRef(0);
  useEffect(() => { generation.current++; setBusy(false); setError(""); return () => { generation.current++; }; }, [identity]);
  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (busy) return;
    if (!identity) { onConnect(); return; }
    const begin = new Date(start).getTime(), finish = new Date(end).getTime();
    if (!name.trim() || !scene || !Number.isFinite(begin) || !Number.isFinite(finish)
      || finish <= Date.now() || finish - begin < 60_000 || finish - begin > 86_400_000) {
      setError("Choose a name, location, and upcoming event lasting between one minute and 24 hours."); return;
    }
    const current = generation.current;
    setBusy(true); setError("");
    try {
      await execute(identity, {type: "create_event", event: {
        name: name.trim(), description: description.trim(), start_at: new Date(begin).toISOString(),
        duration: finish - begin, scene,
      }}, () => current === generation.current);
      if (current === generation.current) setCreated(true);
    } catch(e) {
      if (current === generation.current) setError(e instanceof Error ? e.message : "Event could not be submitted.");
    } finally { if (current === generation.current) setBusy(false); }
  }
  return <><Dialog title="Create an Event" className="event-editor" onClose={onClose}>
    {created ? <div className="dialog-content"><h2>Event submitted</h2><p>&#x201c;{name}&#x201d; is awaiting Decentraland&#x2019;s review. It will appear in Events once approved.</p><button className="primary" onClick={onClose}>Done</button></div>
      : <form className="dialog-content" onSubmit={e => void submit(e)}><h2>Create an Event</h2>
        <fieldset disabled={busy}>
          <label>Name<input required maxLength={150} value={name} onChange={e => setName(e.target.value)} autoFocus /></label>
          <label>Description<textarea maxLength={5000} rows={4} value={description} onChange={e => setDescription(e.target.value)} /></label>
          <div className="event-editor-dates"><label>Starts<input required type="datetime-local" value={start} onChange={e => setStart(e.target.value)} /></label><label>Ends<input required type="datetime-local" value={end} min={start} onChange={e => setEnd(e.target.value)} /></label></div>
          <small className="muted">{Intl.DateTimeFormat().resolvedOptions().timeZone} &#xb7; Up to 24 hours</small>
          <label>Location<button type="button" aria-label={scene ? `Change location: ${destinationLabel(scene)}` : "Choose a place or World"} className="outline-button event-location" onClick={() => setPicker(true)}>{scene ? destinationLabel(scene) : "Choose a place or World"}<span>Choose &#x2192;</span></button></label>
        </fieldset>
        {error && <p className="error" role="alert">{error}</p>}
        <p className="muted">Events appear after Decentraland approves them.</p>
        <button className="primary" disabled={busy}>{busy ? "Submitting\u2026" : identity ? "Submit event" : "Connect to submit"}</button>
      </form>}
  </Dialog>{picker && <ScenePicker purpose="event" onClose={() => setPicker(false)} onSelect={value => {setScene(value); setPicker(false);}} />}</>;
}
