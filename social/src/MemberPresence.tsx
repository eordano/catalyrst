import { useEffect, useState } from 'react';
import { api } from './api';
// ConnectivityStatus values from the shared Social RPC protocol.
const ONLINE = 0, OFFLINE = 1, AWAY = 2;
import { destinationUrl } from './destinations';
import { freshLocation, type LiveLocation } from './location';
import './community-features.css';

// Archipelago reports Explorer presence, not an authoritative offline status.
export function useMemberLocations(addresses: string[], enabled: boolean) {
  const [locations, setLocations] = useState<Record<string, LiveLocation>>({});
  const key = addresses.map(address => address.toLowerCase()).sort().join(',');
  useEffect(() => {
    let active = true; const abort = new AbortController();
    setLocations({});
    if (!enabled || !key) return;
    async function refresh() {
      const wallets = key.split(',');
      try {
        const batches = [];
        for (let i = 0; i < wallets.length; i += 100) batches.push(wallets.slice(i, i + 100));
        const pages = await Promise.all(batches.map(batch => api<{ locations: Record<string, LiveLocation> }>(`/locations?addresses=${batch.join(',')}`, { signal: abort.signal })));
        if (active) setLocations(Object.fromEntries(pages.flatMap(page => Object.entries(page.locations)).filter(([, location]) => freshLocation(location))));
      } catch { if (active) setLocations({}); }
    }
    void refresh(); const timer = setInterval(() => void refresh(), 15000);
    return () => { active = false; abort.abort(); clearInterval(timer); };
  }, [key, enabled]);
  return locations;
}
export function memberIsOnline(location: LiveLocation | undefined, status?: number) { return !!freshLocation(location) || status === ONLINE; }
export function MemberPresence({ location, status }: { location?: LiveLocation; status?: number }) {
  const fresh = freshLocation(location);
  const online = memberIsOnline(location, status);
  const label = fresh ? 'Online in Explorer' : status === ONLINE ? 'Online' : status === AWAY ? 'Away' : status === OFFLINE ? 'Offline' : 'Status unavailable';
  return <span className={`member-presence ${online ? 'is-online' : ''}`}><i />{label}{fresh && <a className="member-presence-link" href={destinationUrl(fresh)} target="_blank" rel="noreferrer">Join &#x2197;</a>}</span>;
}
