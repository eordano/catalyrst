import { useLayoutEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { Avatar, Name } from './Profile';
import type { VoiceState } from './voice';

export function Call({ state, route, accept, leave, mute, deafen }: {
  state: VoiceState;
  route: string;
  accept: () => void;
  leave: () => void;
  mute: () => void;
  deafen: () => void;
}) {
  const [dock, setDock] = useState<HTMLElement | null>(null);
  useLayoutEffect(() => {
    const mobile = window.matchMedia('(max-width: 760px)');
    const place = () => setDock(document.getElementById(mobile.matches
      ? 'mobile-voice' : route.startsWith('#/friends') || route.startsWith('#/dm/')
        ? 'friends-voice' : 'sidebar-voice'));
    place();
    mobile.addEventListener('change', place);
    return () => mobile.removeEventListener('change', place);
  }, [route]);

  if (!dock || (!state.peer && !state.community && !state.error)) return null;
  return createPortal(
    <aside className="call-card" aria-label="Voice call">
      {(state.peer || state.community) && <div className="call-person">
        {state.peer && <Avatar wallet={state.peer} />}
        <div><strong>{state.community
          ? <a href={`#/c/${state.community}/voice`}>The lounge</a>
          : <Name wallet={state.peer!} />}</strong>
          <small>{state.connected ? 'Voice connected' : state.incoming ? 'Incoming call' : state.busy ? 'Connecting\u2026' : 'Calling\u2026'}</small>
        </div>
      </div>}
      <div className="call-controls">
        {state.incoming && state.ringing ? <>
          <button className="primary" disabled={state.busy} onClick={accept}>Accept call</button>
          <button className="outline-button" onClick={leave}>Decline call</button>
        </> : (state.peer || state.community) ? <>
          {state.connected && <>
            <button className="outline-button" disabled={!state.canSpeak || state.hostMuted} onClick={mute}>{state.hostMuted ? 'Muted by host' : state.muted ? 'Unmute call' : 'Mute call'}</button>
            <button className="outline-button" onClick={deafen}>{state.deafened ? 'Hear call' : 'Deafen call'}</button>
          </>}
          <button className="primary" onClick={leave}>{state.community ? 'Leave lounge' : state.connected ? 'End call' : 'Cancel call'}</button>
        </> : <button onClick={leave} aria-label="Dismiss call message">Dismiss</button>}
      </div>
      {state.connected && <button className="outline-button" onClick={()=>window.dispatchEvent(new CustomEvent("social:settings",{detail:"audio"}))}>Audio settings</button>}
      {state.error && <p role="alert">{state.error}</p>}
    </aside>, dock,
  );
}
