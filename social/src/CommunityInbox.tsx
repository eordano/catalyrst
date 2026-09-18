import { useEffect, useRef, useState } from 'react';
import { execute, type Community, type WalletIdentity } from './api';
import { Picture, communityImage } from './Picture';
import './community-features.css';

type CommunityRequest = { id: string; communityId: string; name: string; description: string; membersCount: number; privacy: 'public' | 'private'; status: string };
type RequestPage = { data: { results: CommunityRequest[]; total: number } };
export function CommunityInbox({ identity, onConnect, onOpen, onChanged }: { identity: WalletIdentity | null; onConnect: () => void; onOpen: (community: Community) => void; onChanged?: () => void }) {
  const [tab, setTab] = useState<'invitations' | 'requests'>('invitations');
  const [rows, setRows] = useState<CommunityRequest[]>([]);
  const [total, setTotal] = useState(0), [offset, setOffset] = useState(0), [version, setVersion] = useState(0);
  const [loading, setLoading] = useState(false), [pending, setPending] = useState(''), [error, setError] = useState('');
  const generation = useRef(0);
  useEffect(() => { const current = ++generation.current; setRows([]); setOffset(0); setPending(''); return () => { if (generation.current === current) generation.current++; }; }, [identity, tab]);
  useEffect(() => {
    if (!identity) return;
    let active = true; setLoading(true); setError('');
    execute<RequestPage>(identity, { type: tab === 'invitations' ? 'my_community_invitations' : 'my_join_requests', offset }, () => active)
      .then(r => { if (active) { setRows(all => offset ? [...new Map([...all, ...r.data.results].map(row => [row.id, row])).values()] : r.data.results); setTotal(r.data.total); } })
      .catch(e => { if (active) setError(e.message); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [identity, tab, offset, version]);
  async function resolve(row: CommunityRequest, accept?: boolean) {
    if (!identity || pending) return;
    const current = generation.current; setPending(row.id); setError('');
    try {
      await execute(identity, accept === undefined ? { type: 'cancel_community_request', community_id: row.communityId, request_id: row.id } : { type: 'resolve_request', community_id: row.communityId, request_id: row.id, accept }, () => generation.current === current);
      if (generation.current !== current) return;
      setRows(all => all.map(r => r.id === row.id ? {...r, status: accept === undefined ? 'cancelled' : accept ? 'accepted' : 'rejected'} : r)); onChanged?.();
      if (accept) onOpen({ ...row, id: row.communityId, role: 'member' });
    } catch (e) { if (generation.current === current) setError(e instanceof Error ? e.message : 'Could not update invitation.'); }
    finally { if (generation.current === current) setPending(''); }
  }
  const visible = rows.filter(row => row.status === 'pending');
  return <section className="community-inbox"><header><div><h1>Invitations & requests</h1><p>Invitations and the communities you&#x2019;re waiting to join.</p></div></header>
    {!identity ? <button className="primary" onClick={onConnect}>Connect to see invitations</button> : <><nav className="inbox-tabs" aria-label="Community requests"><button aria-pressed={tab === 'invitations'} onClick={() => { setTab('invitations'); setOffset(0); }}>Invitations</button><button aria-pressed={tab === 'requests'} onClick={() => { setTab('requests'); setOffset(0); }}>Sent requests</button></nav>
      <div className="community-invitations">{visible.map(row => <article className="community-invitation" key={row.id}><div className="invitation-picture"><Picture src={communityImage(row.communityId)} fallback={row.name.slice(0, 2)} /></div><div className="invitation-copy"><button className="invitation-title" onClick={() => onOpen({ ...row, id: row.communityId })}>{row.name}</button><p>{row.description}</p><small>{row.membersCount} members &#xb7; {tab === 'invitations' ? 'You\u2019re invited' : 'Awaiting approval'}</small></div><div className="invitation-actions">{tab === 'invitations' ? <><button className="primary" disabled={!!pending} onClick={() => void resolve(row, true)}>{pending === row.id ? 'Updating\u2026' : 'Accept invitation'}</button><button className="outline-button" disabled={!!pending} onClick={() => void resolve(row, false)}>Decline</button></> : <button className="outline-button" disabled={!!pending} onClick={() => void resolve(row)}>Cancel request</button>}</div></article>)}</div>
      {loading && <p role="status">Loading&#x2026;</p>}{!loading && !error && !visible.length && <div className="inbox-empty"><h2>{tab === 'invitations' ? 'You\u2019re all caught up.' : 'No pending requests.'}</h2><p>{tab === 'invitations' ? 'Community invitations will appear here.' : 'Ask to join a private community to get started.'}</p></div>}
      {rows.length < total && !loading && <button className="outline-button" onClick={() => setOffset(offset + 100)}>Load more</button>}
      {error && <div role="alert"><p>{error}</p><button className="outline-button" onClick={() => setVersion(v => v + 1)}>Try again</button></div>}
    </>}
  </section>;
}

export function PendingCommunityRequest({ identity, community, onChanged }: { identity: WalletIdentity; community: Community; onChanged: () => void }) {
  const [busy, setBusy] = useState(false), [error, setError] = useState('');
  const generation = useRef(0); useEffect(() => { generation.current++; setBusy(false); setError(''); return () => { generation.current++; }; }, [identity, community.id]);
  async function cancel() {
    if (busy) return; const current = generation.current; const active = () => generation.current === current; setBusy(true); setError('');
    try {
      let offset = 0, found: CommunityRequest | undefined;
      while (active()) {
        const page = await execute<RequestPage>(identity, { type: 'my_join_requests', offset }, active);
        found = page.data.results.find(row => row.communityId === community.id && row.status === 'pending');
        if (found || offset + page.data.results.length >= page.data.total || !page.data.results.length) break;
        offset += page.data.results.length;
      }
      if (found) await execute(identity, { type: 'cancel_community_request', community_id: community.id, request_id: found.id }, active);
      if (active()) onChanged();
    } catch (e) { if (active()) setError(e instanceof Error ? e.message : 'Could not cancel request.'); }
    finally { if (active()) setBusy(false); }
  }
  return <div className="pending-community-request"><p role="status">Your request is with the community moderators.</p><div><button className="outline-button" disabled={busy} onClick={() => void cancel()}>{busy ? 'Cancelling\u2026' : 'Cancel request'}</button></div>{error && <p role="alert">{error}</p>}</div>;
}
