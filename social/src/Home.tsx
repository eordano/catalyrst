import type { Community, WalletIdentity } from './api';
import { shortWallet } from './api';
import type { FriendsState } from './friends';
import { Picture, communityImage } from './Picture';
import { Icon } from './Icon';
import { EventsPreview } from './Events';
import { WorldsPreview } from './Worlds';
import './home.css';

export function Home({ identity, friends, communities, joined, loading, error, onConnect, onRetry, onCreate }: {
  identity: WalletIdentity | null;
  friends: FriendsState;
  communities: Community[];
  joined: Community[];
  loading: boolean;
  error: string;
  onConnect: () => void;
  onRetry: () => void;
  onCreate: () => void;
}) {
  const available = friends.friends.filter(friend => friend.available || friend.status === 0 || friend.location);
  const people = [...friends.friends].sort((a, b) =>
    Number(b.available || b.status === 0 || !!b.location) - Number(a.available || a.status === 0 || !!a.location)
    || (a.name || a.address).localeCompare(b.name || b.address));
  const groups = joined.length ? joined : communities;
  return <section className="social-home" aria-label="Home">
    <header className="home-heading">
      <div><h1>A little closer, wherever you are.</h1><p>Your people. Your communities. Your next adventure.</p></div>
      {identity ? <button className="outline-button" onClick={() => window.dispatchEvent(new Event('social:add-friend'))}><Icon name="plus" />Add a friend</button>
        : <button className="primary" onClick={onConnect}>Connect wallet</button>}
    </header>
    <div className="home-columns">
      <section className="home-panel home-people" aria-labelledby="home-people-title">
        <header><h2 id="home-people-title"><Icon name="people" />Friends</h2><a href="#/friends">Open inbox <span aria-hidden="true">&#x2197;</span></a></header>
        <div className="home-panel-body">
          {!identity ? <div className="home-empty"><div className="home-orbit"><Icon name="people" /></div><h3>Good company starts here.</h3><p>Find your friends, see who&#x2019;s around, and pick up a conversation.</p><button className="outline-button" onClick={onConnect}>Find your people</button></div>
            : <><div className="home-section-note"><span className={`presence-dot ${available.length ? 'is-online' : ''}`} />{friends.ready ? `${available.length} online` : friends.error ? 'Connection interrupted' : 'Connecting friends\u2026'}<a href="#/friends/requests">Requests</a></div>
              {friends.error && <p className="home-inline-error" role="status">{friends.error} <a href="#/friends">Open friends</a></p>}
              {people.slice(0, 5).map(friend => {
                const online = friend.available || friend.status === 0 || !!friend.location;
                return <a className="home-friend" href={`#/dm/${friend.address.toLowerCase()}`} key={friend.address}>
                  <span className="home-avatar"><Picture src={friend.profilePictureUrl} fallback={(friend.name || friend.address.slice(2)).slice(0, 2)} /><i className={online ? 'is-online' : friend.status === 2 ? 'is-away' : ''} /></span>
                  <span><strong>{friend.name || shortWallet(friend.address)}</strong><small>{friend.location ? `Exploring ${friend.location.x}, ${friend.location.y}` : online ? 'Online' : friend.status === 2 ? 'Away' : friend.status === 1 ? 'Offline' : 'Status unavailable'}</small></span>
                  <Icon name="chat" />
                </a>;
              })}
              {friends.ready && !people.length && <div className="home-empty"><h3>Make room for your people.</h3><p>Add a friend to start chatting and exploring together.</p><button className="outline-button" onClick={() => window.dispatchEvent(new Event('social:add-friend'))}>Add a friend</button></div>}
            </>}
        </div>
        <footer><a href="#/nearby"><Icon name="globe" />Find friends out exploring</a></footer>
      </section>
      <section className="home-panel home-communities" aria-labelledby="home-communities-title">
        <header><h2 id="home-communities-title"><Icon name="chat" />Communities</h2><a href="#/discover">Discover <span aria-hidden="true">&#x2197;</span></a></header>
        <div className="home-panel-body">
          <div className="home-section-note">{joined.length ? 'Your communities' : 'Find your kind of people'}<a href="#/invitations">Invitations</a></div>
          {loading && !groups.length && <p className="home-loading" role="status">Loading communities&#x2026;</p>}
          {error && !groups.length && <div className="home-inline-error" role="alert"><p>{error}</p><button onClick={onRetry}>Try again</button></div>}
          {groups.slice(0, 4).map(community => <a className="home-community" href={`#/c/${community.id}/general`} key={community.id}>
            <span className="home-community-image"><Picture src={community.thumbnails?.raw || communityImage(community.id)} fallback={community.name.slice(0, 2)} /></span>
            <span><strong>{community.name}</strong><small>{community.membersCount.toLocaleString()} members{community.privacy === 'private' ? ' \u00b7 Private' : ''}</small><p>{community.description}</p></span>
          </a>)}
          {!loading && !error && !groups.length && <div className="home-empty"><h3>A place to belong.</h3><p>Start a community and bring your people together.</p><button className="outline-button" onClick={onCreate}>Create a community</button></div>}
        </div>
        <footer><a href="#/mine"><Icon name="people" />Your communities</a><button onClick={onCreate} aria-label="Create community from home"><Icon name="plus" /></button></footer>
      </section>
      <section className="home-panel home-events" aria-labelledby="home-events-title">
        <header><h2 id="home-events-title"><Icon name="calendar" />Events</h2><a href="#/events">All events <span aria-hidden="true">&#x2197;</span></a></header>
        <div className="home-panel-body"><EventsPreview friendsState={friends} identity={identity} onConnect={onConnect} /></div>
      </section>
    </div>
    <section className="home-worlds" aria-labelledby="home-worlds-title"><header><div><h2 id="home-worlds-title">A world away, one click away.</h2><p>New places to meet. New reasons to stay.</p></div><a className="outline-button" href="#/worlds">Explore Worlds <Icon name="external" /></a></header><WorldsPreview /></section>
  </section>;
}
