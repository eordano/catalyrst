import { useRef, useEffect, useState } from 'react';
import { execute, type Community, type WalletIdentity } from './api';
import { Dialog } from './Dialog';
import { Picture, communityImage, refreshCommunityPicture } from './Picture';
import './community-features.css';

export function CommunityEditor({ community, identity, onClose, onSaved }: { community?: Community; identity: WalletIdentity; onClose: () => void; onSaved: (community: Community) => void }) {
  const [name, setName] = useState(community?.name || '');
  const [description, setDescription] = useState(community?.description || '');
  const [privacy, setPrivacy] = useState<'public' | 'private'>(community?.privacy || 'public');
  const [visibility, setVisibility] = useState<'all' | 'unlisted'>(community?.visibility || 'all');
  const [picture, setPicture] = useState<{ thumbnail: string; preview: string } | null>(null);
  const [busy, setBusy] = useState(false), [reading, setReading] = useState(false), [error, setError] = useState('');
  const active = useRef(true), fileGeneration = useRef(0);
  useEffect(() => { active.current = true; return () => { active.current = false; fileGeneration.current++; }; }, []);
  async function choosePicture(file?: File) {
    const generation = ++fileGeneration.current; setError('');
    if (!file) return;
    if (!['image/png', 'image/jpeg', 'image/webp', 'image/gif'].includes(file.type) || file.size < 1024 || file.size > 500 * 1024) { setError('Choose a PNG, JPEG, GIF or WebP picture between 1 KB and 500 KB.'); return; }
    setReading(true);
    try {
      const preview = await new Promise<string>((resolve, reject) => { const reader = new FileReader(); reader.onload = () => resolve(String(reader.result)); reader.onerror = () => reject(new Error('This picture could not be read.')); reader.readAsDataURL(file); });
      // Verify the browser can decode the file before showing the upload preview.
      const image = new Image(); image.src = preview; await image.decode();
      if (active.current && generation === fileGeneration.current) setPicture({ preview, thumbnail: preview.slice(preview.indexOf(',') + 1) });
    } catch { if (active.current && generation === fileGeneration.current) setError('This picture could not be opened. Choose another file.'); }
    finally { if (active.current && generation === fileGeneration.current) setReading(false); }
  }
  async function save(event: React.FormEvent) {
    event.preventDefault(); if (busy || reading) return; setBusy(true); setError('');
    try {
      const fields = { name, description, privacy, visibility, ...(picture ? { thumbnail: picture.thumbnail } : {}) };
      const response = await execute<{ data: Community }>(identity, community ? { type: 'update_community', community_id: community.id, ...fields } : { type: 'create_community', ...fields }, () => active.current);
      if (active.current) { if (picture) refreshCommunityPicture(response.data.id || community?.id || ''); onSaved(response.data); }
    } catch (e) { if (active.current) setError(e instanceof Error ? e.message : 'Could not save community.'); }
    finally { if (active.current) setBusy(false); }
  }
  return <Dialog title={community ? 'Community settings' : 'Create a community'} onClose={onClose}><form className="dialog-content community-editor" onSubmit={event => void save(event)}><h2>{community ? 'Make it your own.' : 'Make room for your people.'}</h2>
    <div className="community-picture-editor"><div className="community-picture-preview">{picture ? <img src={picture.preview} alt="Community picture preview" /> : <Picture src={community && communityImage(community.id)} fallback={name.slice(0, 2) || '\u25c7'} />}</div><div><label>Community picture<input aria-label="Community picture" type="file" accept="image/png,image/jpeg,image/webp,image/gif" disabled={busy || reading} onChange={event => void choosePicture(event.target.files?.[0])} /></label><small>PNG, JPEG, GIF or WebP &#xb7; up to 500 KB</small>{picture && <button type="button" className="text-button" onClick={() => setPicture(null)} disabled={busy}>Undo picture change</button>}{reading && <p role="status">Opening picture&#x2026;</p>}</div></div>
    <label>Name<input required maxLength={30} value={name} onChange={event => setName(event.target.value)} placeholder="Your community" /></label><label>Description<textarea required maxLength={500} value={description} onChange={event => setDescription(event.target.value)} placeholder="What brings you together?" /></label><label>Privacy<select value={privacy} onChange={event => setPrivacy(event.target.value as 'public' | 'private')}><option value="public">Public &#x2014; anyone can join</option><option value="private">Private &#x2014; approval required</option></select></label><label>Discovery<select value={visibility} onChange={event => setVisibility(event.target.value as 'all' | 'unlisted')}><option value="all">Show in Discover</option><option value="unlisted">Unlisted &#x2014; share an invite link</option></select></label>{error && <p role="alert">{error}</p>}<button className="primary" disabled={busy || reading}>{busy ? 'Saving\u2026' : community ? 'Save changes' : 'Create community'}</button>
  </form></Dialog>;
}
