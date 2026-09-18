import { useState } from 'react';
import type { Friend } from './friends';
import { freshLocation } from './location';
import { destinationUrl } from './destinations';
import { Picture } from './Picture';
import { shortWallet } from './api';
import './friends-map.css';

export function FriendsMap({ friends }: { friends: Friend[] }) {
  const [selected, setSelected] = useState('');
  const groups = new Map<string, Friend[]>();
  for (const friend of friends) {
    const location = freshLocation(friend.location);
    if (!location || location.world) continue;
    const key = `${location.x}, ${location.y}`;
    groups.set(key, [...(groups.get(key) || []), friend]);
  }
  const current = groups.get(selected);
  return <section className="friends-map" aria-label="Friends map">
    <header><div><h2>Friends in Genesis City</h2><p>Live locations. Select a marker to join.</p></div><span>{[...groups.values()].reduce((n, group) => n + group.length, 0)} exploring</span></header>
    <div className="friends-map-canvas">
      <svg viewBox="0 0 320 320" preserveAspectRatio="none" aria-hidden="true"><defs><pattern id="friends-parcel-grid" width="20" height="20" patternUnits="userSpaceOnUse"><path d="M20 0H0V20" fill="none" stroke="#ffffff10" /></pattern></defs><rect width="320" height="320" fill="url(#friends-parcel-grid)" /><path d="M160 0V320M0 160H320" stroke="#ffffff30" strokeDasharray="3 5" /><text x="164" y="174" fill="#aaa4b4" fontSize="8">0, 0</text><text x="6" y="12" fill="#aaa4b4" fontSize="8">150</text><text x="6" y="314" fill="#aaa4b4" fontSize="8">&#x2212;150</text></svg>
      {[...groups.entries()].map(([key, group]) => <button key={key} className={`friends-map-marker ${selected === key ? 'selected' : ''}`} style={{left: `clamp(20px, ${5 + (group[0].location!.x + 150) / 300 * 90}%, calc(100% - 20px))`, top: `clamp(20px, ${5 + (150 - group[0].location!.y) / 300 * 90}%, calc(100% - 20px))`}} aria-label={`${group.map(f => f.name || shortWallet(f.address)).join(', ')} at ${key}`} aria-pressed={selected === key} onClick={() => setSelected(key)}><Picture src={group[0].profilePictureUrl} fallback={(group[0].name || '?').slice(0, 1)} />{group.length > 1 && <b>{group.length}</b>}</button>)}
      {!groups.size && <div className="friends-map-empty">No friends are sharing a fresh Genesis City location right now.</div>}
    </div>
    {current && <div className="friends-map-detail"><strong>{selected}</strong>{current.map(friend => <div key={friend.address}><a href={`#/dm/${friend.address}`}>{friend.name || shortWallet(friend.address)}</a><a className="outline-button" href={destinationUrl(friend.location!)} target="_blank" rel="noreferrer">Join</a></div>)}</div>}
    <small>Coordinate map &#xb7; Worlds aren&#x2019;t shown on this grid.</small>
  </section>;
}
