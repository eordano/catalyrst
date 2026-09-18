import { useEffect, useRef, useState } from 'react';
import { api, execute, type Community, type WalletIdentity } from './api';
import { Picture } from './Picture';
import { destinationUrl, worldName } from './destinations';
import './community-features.css';

type Place = { id: string; title?: string; description?: string; image?: string; base_position?: string; world_name?: string; world?: boolean };
type Entry = Place;
function visit(place: Place) {
  const world = worldName(place.world_name || place.id || '');
  if (world) return destinationUrl({ x: 0, y: 0, world });
  if (place.world) return null;
  const coordinates = place.base_position?.split(',').map(Number);
  return coordinates?.length === 2 && coordinates.every(n => Number.isInteger(n) && Math.abs(n) <= 150) ? destinationUrl({ x: coordinates[0], y: coordinates[1] }) : null;
}
function Hangout({ entry, canManage, busy, onRemove }: { entry: Entry; canManage: boolean; busy: boolean; onRemove: () => void }) {
  const [place, setPlace] = useState<Place | null>(entry.title ? entry : null), [error, setError] = useState(''), [revision, setRevision] = useState(0);
  useEffect(() => { const abort = new AbortController(); setError(''); setPlace(entry.title ? entry : null); if (entry.title && visit(entry)) return; api<{ data: Place }>(`/place/${encodeURIComponent(entry.id)}`, { signal: abort.signal }).then(r => setPlace(r.data)).catch(e => { if (!abort.signal.aborted) { setError(e.message); const world = worldName(entry.id); if (world) setPlace({id:entry.id,title:world,world:true,world_name:world}); } }); return () => abort.abort(); }, [entry.id, revision]);
  const url = place && visit(place);
  return <article className="community-hangout"><div className="hangout-art"><Picture src={place?.image} fallback="&#x2197;" /></div><div><h3>{place?.title || (error ? 'Place unavailable' : 'Loading place\u2026')}</h3>{place?.description && <p>{place.description}</p>}<small>{place?.world_name || place?.base_position}</small>{error && <small>Place details unavailable. </small>}{error && <button className="text-button" onClick={() => setRevision(v => v + 1)}>Try again</button>}</div><div className="hangout-actions">{url && <a className="primary" href={url} target="_blank" rel="noreferrer">Visit &#x2197;</a>}{canManage && <button className="outline-button" disabled={busy} onClick={onRemove}>Remove</button>}</div></article>;
}
export function CommunityHangouts({ community, identity }: { community: Community; identity: WalletIdentity }) {
  const [entries, setEntries] = useState<Entry[]>([]), [total, setTotal] = useState(0), [offset, setOffset] = useState(0), [revision, setRevision] = useState(0);
  const [kind, setKind] = useState<'places' | 'worlds'>('places');
  const [search, setSearch] = useState(''), [results, setResults] = useState<Place[]>([]), [searching, setSearching] = useState(false), [searchError, setSearchError] = useState('');
  const [loading, setLoading] = useState(false), [busy, setBusy] = useState(false), [error, setError] = useState('');
  const generation = useRef(0); useEffect(() => { generation.current++; return () => { generation.current++; }; }, [community.id, identity]);
  const canManage = ['owner', 'moderator'].includes(community.role || '');
  useEffect(() => { setEntries([]); setOffset(0); }, [community.id]);
  useEffect(() => { let active = true; setLoading(true); setError(''); execute<{ data: { results: Entry[]; total: number } }>(identity, { type: 'community_places', community_id: community.id, offset }, () => active).then(r => { if (active) { setEntries(all => offset ? [...new Map([...all, ...r.data.results].map(entry => [entry.id, entry])).values()] : r.data.results); setTotal(r.data.total); } }).catch(e => { if (active) setError(e.message); }).finally(() => { if (active) setLoading(false); }); return () => { active = false; }; }, [community.id, identity, offset, revision]);
  useEffect(() => { const abort = new AbortController(); setResults([]); setSearchError(''); if (!search.trim()) { setSearching(false); return; } setSearching(true); const timer = setTimeout(() => { api<{ data: Place[] }>(`/${kind}?search=${encodeURIComponent(search)}`, { signal: abort.signal }).then(r => setResults(r.data)).catch(e => { if (!abort.signal.aborted) setSearchError(e.message); }).finally(() => { if (!abort.signal.aborted) setSearching(false); }); }, 250); return () => { abort.abort(); clearTimeout(timer); }; }, [search, kind]);
  async function update(id: string, remove: boolean) {
    if (busy) return; const current = generation.current; setBusy(true); setError('');
    try { await execute(identity, remove ? { type: 'remove_community_place', community_id: community.id, place_id: id } : { type: 'add_community_places', community_id: community.id, place_ids: [id] }, () => generation.current === current); if (generation.current === current) { setSearch(''); setOffset(0); setRevision(v => v + 1); } }
    catch (e) { if (generation.current === current) setError(e instanceof Error ? e.message : 'Hangout could not be updated.'); }
    finally { if (generation.current === current) setBusy(false); }
  }
  return <section className="community-hangouts"><header><h2>Hangouts</h2><p>Places where {community.name} gets together.</p></header>{canManage && <div className="hangout-search"><div className="hangout-search-kinds"><button type="button" aria-pressed={kind === 'places'} onClick={() => setKind('places')}>Places</button><button type="button" aria-pressed={kind === 'worlds'} onClick={() => setKind('worlds')}>Worlds</button></div><label>Add a hangout<input type="search" aria-label="Find a community hangout" placeholder={kind === 'worlds' ? 'Search worlds you own or manage' : 'Search places you own or manage'} maxLength={200} value={search} onChange={e => setSearch(e.target.value)} /></label><small>You need permission to manage the place in Decentraland.</small>{searching && <p role="status">Finding places&#x2026;</p>}{results.map(place => <div className="hangout-result" key={place.id}><span>{place.title || place.id}<small>{place.world_name || place.base_position}</small></span><button className="outline-button" disabled={busy || entries.some(entry => entry.id === place.id)} onClick={() => void update(place.id, false)}>{entries.some(entry => entry.id === place.id) ? 'Added' : 'Add hangout'}</button></div>)}{searchError && <p role="alert">{searchError}</p>}{search.trim() && !searching && !searchError && !results.length && <p>No places found.</p>}</div>}
    <div className="community-hangout-list">{entries.map(entry => <Hangout key={entry.id} entry={entry} canManage={canManage} busy={busy} onRemove={() => void update(entry.id, true)} />)}</div>{loading && <p role="status">Loading hangouts&#x2026;</p>}{!loading && !error && !entries.length && <p className="muted">No hangouts added yet.</p>}{entries.length < total && !loading && <button className="outline-button" onClick={() => setOffset(offset + 100)}>More hangouts</button>}{error && <div role="alert"><p>{error}</p><button className="outline-button" onClick={() => setRevision(v => v + 1)}>Refresh hangouts</button></div>}
  </section>;
}
