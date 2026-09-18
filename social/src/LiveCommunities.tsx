import { useEffect, useState } from 'react';
import { execute, type WalletIdentity } from './api';
import { Picture, communityImage } from './Picture';
import './community-features.css';

type ActiveChat = { communityId: string; communityName: string; communityImage?: string; isMember: boolean; participantCount: number; moderatorCount: number };
export function LiveCommunities({ identity, onConnect, onJoin }: { identity: WalletIdentity | null; onConnect: () => void; onJoin: (communityId: string) => void }) {
  const [chats, setChats] = useState<ActiveChat[]>([]), [error, setError] = useState(''), [loading, setLoading] = useState(false), [loaded, setLoaded] = useState(false), [revision, setRevision] = useState(0);
  useEffect(() => {
    let active = true, running = false; setChats([]); setError(''); setLoaded(false);
    if (!identity) return;
    async function refresh() {
      if (running || !identity) return; running = true; setLoading(true);
      try { const response = await execute<{ data: { activeChats: ActiveChat[] } }>(identity, { type: 'active_community_voice_chats' }, () => active); if (active) { setChats(response.data.activeChats.filter(chat => chat.participantCount > 0)); setError(''); } }
      catch (e) { if (active) { setChats([]); setError(e instanceof Error ? e.message : 'Live conversations could not be loaded.'); } }
      finally { running = false; if (active) { setLoading(false); setLoaded(true); } }
    }
    void refresh(); const timer = identity.canSignSilently ? setInterval(() => void refresh(), 20000) : undefined;
    return () => { active = false; clearInterval(timer); };
  }, [identity, revision]);
  if (identity && loaded && !error && !chats.length) return null;
  return <section className="live-communities" aria-label="Live community conversations"><header><div><h2>Live now</h2><p>Drop into a community conversation.</p></div>{identity && <button className="outline-button" disabled={loading} onClick={() => setRevision(v => v + 1)}>Refresh</button>}</header>
    {!identity ? <button className="outline-button" onClick={onConnect}>Connect to see live conversations</button> : <><div className="live-community-grid">{chats.map(chat => <article className="live-community-card" key={chat.communityId}><div className="live-community-picture"><Picture src={chat.communityImage || communityImage(chat.communityId)} fallback={chat.communityName.slice(0, 2)} /><span aria-label="Live" /></div><div><h3>{chat.communityName}</h3><p>{chat.participantCount} {chat.participantCount === 1 ? 'person' : 'people'} in voice</p></div><button className="primary" onClick={() => onJoin(chat.communityId)}>Open voice chat</button></article>)}</div>{loading && !chats.length && <p role="status">Finding conversations&#x2026;</p>}{error && <p role="alert">{error}</p>}</>}
  </section>;
}
