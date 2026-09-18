import { useState, useRef, useEffect } from "react";
import type { Base, Wearable } from "../Backpack.types";
import { SLOTS } from "../Backpack.types";
import { baseItemUrn } from "../../../data/catalyst/backpack";

export type LobbyLook = Base & { wearables: string[]; emotes: string[] };

export default function LobbyWardrobe({ look, catalog, pending, error, onChange, onSave, onCancel, onRetry }: {
  look: LobbyLook; catalog: Wearable[]; pending: boolean; error: boolean;
  onChange: (look: LobbyLook) => void; onSave: () => void; onCancel: () => void; onRetry: () => void;
}) {
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => { heading.current?.focus({ preventScroll: true }); }, []);
  const [category, setCategory] = useState("all");
  const [search, setSearch] = useState("");
  const equipped = new Set(look.wearables.map(baseItemUrn));
  const items = catalog.filter(item => (category === "all" || item.category === category)
    && (item.name || "").toLowerCase().includes(search.trim().toLowerCase()));
  function equip(item: Wearable) {
    if (item.category === "body_shape") { onChange({ ...look, bodyShape: item.urn, wearables: look.wearables.filter(urn => catalog.find(w => w.urn === baseItemUrn(urn))?.category !== "body_shape") }); return; }
    const wearables = look.wearables.filter(urn => {
      if (baseItemUrn(urn) === item.urn) return false;
      return equipped.has(item.urn) || catalog.find(w => w.urn === baseItemUrn(urn))?.category !== item.category;
    });
    if (!equipped.has(item.urn)) wearables.push(item.urn);
    onChange({ ...look, wearables });
  }
  return <section className="lh__wardrobe lh__panel" aria-label="Edit avatar">
    <div className="lh__panel-heading"><h2 tabIndex={-1} ref={heading}>Backpack</h2><button className="lh__section-link" onClick={onCancel}>Cancel</button></div>
    <p className="lh__note">Try a new look. Save when it feels like you.</p>
    <div className="lh__colors">{([['skinColor', 'Skin'], ['hairColor', 'Hair'], ['eyeColor', 'Eyes']] as const).map(([field, label]) => <label key={field}><input type="color" aria-label={`${label} color`} value={look[field]} onChange={event => onChange({ ...look, [field]: event.target.value })} />{label}</label>)}</div>
    <div className="lh__wardrobe-filters"><input type="search" aria-label="Search wearables" placeholder="Search your wearables" value={search} onChange={event => setSearch(event.target.value)} /><select aria-label="Wearable category" value={category} onChange={event => setCategory(event.target.value)}><option value="all">All wearables</option>{SLOTS.filter(slot => catalog.some(item => item.category === slot.id)).map(slot => <option key={slot.id} value={slot.id}>{slot.label}</option>)}</select></div>
    {pending && <p role="status" className="lh__note">Loading your backpack&#x2026;</p>}
    {error && <p role="alert" className="lh__note">Couldn&#x2019;t load wearables. <button className="lh__section-link" onClick={onRetry}>Retry</button></p>}
    <div className="lh__wardrobe-items">{items.map(item => <button key={item.urn} data-wearable-urn={item.urn} aria-label={`${equipped.has(item.urn) ? 'Unequip' : 'Equip'} ${item.name}`} aria-pressed={item.category === "body_shape" ? look.bodyShape === item.urn : equipped.has(item.urn)} onClick={() => equip(item)}>{item.thumbnail && <img src={item.thumbnail} alt="" loading="lazy" />}<span>{item.name}</span></button>)}</div>
    {!pending && !error && !items.length && <p className="lh__note">No wearables match. Try another category or search.</p>}
    <footer className="lh__panel-actions"><button className="lh__jump" onClick={onSave}>Save avatar</button><button className="lh__section-link" onClick={onCancel}>Discard changes</button></footer>
  </section>;
}
