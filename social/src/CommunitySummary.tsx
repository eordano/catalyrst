import { useEffect, useState } from 'react';
import { execute, shortWallet, type Community, type Post, type WalletIdentity } from './api';
import { Picture, communityImage } from './Picture';
import { Icon } from './Icon';
import { CommunityHangouts } from './CommunityHangouts';
import { fullMessageTime, isoTime, messageTime } from './message-time';
import './community-features.css';

const roles: Record<string, string> = { owner: 'You own this community', moderator: 'You moderate this community', member: 'You\u2019re a member' };

export function CommunitySummary({ community, identity, onOpen }: { community: Community; identity: WalletIdentity; onOpen: (target: string) => void }) {
  const [post, setPost] = useState<Post | null>(null), [error, setError] = useState(''), [loading, setLoading] = useState(true);
  useEffect(() => {
    let active = true; setPost(null); setError(''); setLoading(true);
    execute<{ data: { posts: Post[] } }>(identity, { type: 'community_posts', community_id: community.id }, () => active)
      .then(r => { if (active) setPost(r.data.posts.reduce<Post | null>((latest, p) => !latest || Date.parse(p.createdAt) > Date.parse(latest.createdAt) ? p : latest, null)); })
      .catch(e => { if (active && !(e instanceof DOMException)) setError(e instanceof Error ? e.message : 'Announcements could not be loaded.'); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [community.id, identity]);
  return <section className="community-summary" aria-label="Community summary">
    <div className="community-banner"><Picture src={community.thumbnails?.raw || communityImage(community.id)} fallback={community.name.slice(0, 2)} /></div>
    <header>
      <h1>{community.name}</h1>
      <p className="summary-facts">{community.membersCount.toLocaleString()} members &#xb7; {community.privacy === 'private' ? 'Private' : 'Public'}{roles[community.role || ''] ? <> &#xb7; {roles[community.role || '']}</> : null}</p>
      {community.description && <p>{community.description}</p>}
      <div className="summary-actions">
        <button className="primary" onClick={() => onOpen('general')}>Open General</button>
        <button className="outline-button" onClick={() => onOpen('announcements')}><Icon name="bell" />Announcements</button>
        <button className="outline-button" onClick={() => onOpen('members')}><Icon name="people" />Members</button>
      </div>
    </header>
    <section className="summary-latest" aria-labelledby="summary-latest-title">
      <h2 id="summary-latest-title">Latest announcement</h2>
      {post ? <article title={fullMessageTime(post.createdAt)}><header><strong>{post.authorName || shortWallet(post.authorAddress)}</strong><time dateTime={isoTime(post.createdAt)}>{messageTime(post.createdAt, false)}</time></header><p>{post.content}</p></article>
        : <p className="muted" role={loading ? 'status' : undefined}>{loading ? 'Loading\u2026' : error || 'Nothing announced yet.'}</p>}
    </section>
    <CommunityHangouts community={community} identity={identity} />
  </section>;
}
