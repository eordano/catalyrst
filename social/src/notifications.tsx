import { useCallback, useEffect, useRef, useState } from 'react';
import './notifications.css';
const key = 'dcl.social.notifications';
export type Activity = { id: string; title: string; body: string; href: string; createdAt: number; kind: 'event' | 'message' | 'friend'; read?: boolean };
function read(key: string) { try { return localStorage.getItem(key); } catch { return null; } }
function save(key: string, value: string) { try { localStorage.setItem(key,value); } catch {} }
function desktop(item: Pick<Activity,'id'|'title'|'body'|'href'>) {
  if (typeof Notification === 'undefined' || Notification.permission !== 'granted' || read(key) !== 'on') return;
  try { const n = new Notification(item.title, { body:item.body, tag:item.id }); n.onclick = () => { window.focus(); window.location.hash=item.href; n.close(); }; setTimeout(()=>n.close(),10000); } catch {}
}
export function NotificationPreference() {
  const supported = typeof Notification !== 'undefined';
  const [enabled,setEnabled] = useState(() => supported && Notification.permission === 'granted' && read(key) === 'on');
  const [error,setError] = useState('');
  return <><label className="preference-row"><span><strong>Desktop notifications</strong><small>Messages and friends attending events, while dcl.social is open.</small></span><input type="checkbox" aria-label="Desktop notifications" disabled={!supported} checked={enabled} onChange={async e => { setError(''); if (!e.target.checked) {save(key,'off');setEnabled(false);return;} try { const permission = await Notification.requestPermission(); if(permission !== 'granted'){setError('Allow notifications in your browser settings to enable them.');return;}save(key,'on');setEnabled(true); }catch{setError('Notifications are unavailable in this browser.');} }} /></label>{error && <small role="status">{error}</small>}</>;
}
export function notifyMessage(name: string, peer: string) {
  const item:Activity = { id:`message:${peer}:${Date.now()}`,title:`New message from ${name}`,body:'Open your conversation.',href:`#/dm/${peer}`,createdAt:Date.now(),kind:'message' };
  window.dispatchEvent(new CustomEvent('social:activity',{detail:item}));
}
export function useActivity(wallet?: string) {
  const storageKey = 'dcl.social.activity.' + (wallet?.toLowerCase() || 'guest');
  const current = useRef<{key:string;items:Activity[]}>({key:storageKey,items:[]});
  const [state,setState] = useState<{key:string;items:Activity[]}>({key:storageKey,items:[]});
  useEffect(() => {
    let items:Activity[] = [];
    try { const raw=JSON.parse(read(storageKey)||'[]'); if(Array.isArray(raw)) items=raw.filter(x=>typeof x?.id==='string'&&typeof x.title==='string'&&typeof x.body==='string'&&typeof x.href==='string'&&/^#\/(events|dm|friends)(\/|$)/.test(x.href)&&Number.isFinite(x.createdAt)).slice(0,100); } catch {}
    current.current={key:storageKey,items:wallet?items:[]};
    setState(current.current);
  },[storageKey,wallet]);
  const notify=useCallback((item:Activity) => {
    if(!wallet) return;
    const old=current.current;
    if(old.key!==storageKey || old.items.some(x=>x.id===item.id)) return;
    const items=[item,...old.items].slice(0,100);
    current.current={key:storageKey,items};save(storageKey,JSON.stringify(items));setState(current.current);
    desktop(item);
  },[storageKey,wallet]);
  useEffect(()=>{const receive=(e:Event)=>notify((e as CustomEvent<Activity>).detail);window.addEventListener('social:activity',receive);return()=>window.removeEventListener('social:activity',receive);},[notify]);
  const update=useCallback((fn:(items:Activity[])=>Activity[])=>setState(old=>{if(old.key!==storageKey)return old;const items=fn(old.items);current.current={key:storageKey,items};save(storageKey,JSON.stringify(items));return current.current;}),[storageKey]);
  return {items:state.key===storageKey?state.items:[],notify,markRead:(id?:string)=>update(items=>items.map(x=>!id||x.id===id?{...x,read:true}:x)),clear:()=>update(()=>[])};
}
export function ActivityInbox({items,markRead,clear}:{items:Activity[];markRead:(id?:string)=>void;clear:()=>void}) {
  return <section className="activity-inbox"><header><div><h1>Notifications</h1><p>Messages and plans with your people.</p></div><div><button className="outline-button" disabled={!items.some(x=>!x.read)} onClick={()=>markRead()}>Mark all read</button><button className="outline-button" disabled={!items.length} onClick={clear}>Clear</button></div></header><p className="activity-note">Event activity updates while dcl.social is open. Notifications are saved on this device.</p>{!items.length?<div className="activity-empty">You&#x2019;re all caught up.</div>:<ol>{items.map(item=><li key={item.id} className={item.read?'':'unread'}><a href={item.href} onClick={()=>markRead(item.id)}><div><strong>{item.title}</strong><p>{item.body}</p></div><time dateTime={new Date(item.createdAt).toISOString()}>{new Date(item.createdAt).toLocaleString(undefined,{month:'short',day:'numeric',hour:'numeric',minute:'2-digit'})}</time></a></li>)}</ol>}</section>;
}

export function FriendRequestActivity({wallet,requests,ready,onNotify}:{wallet:string;requests?:import('./friends').FriendsState['requests'];ready:boolean;onNotify:(item:Activity)=>void}) {
  const baseline=useRef<{wallet:string;ids:Set<string>} | null>(null);
  useEffect(()=>{
    if(!ready || !requests) return;
    const ids=new Set(requests.map(r=>r.id));
    if(baseline.current?.wallet===wallet) for(const request of requests) {
      if(!baseline.current.ids.has(request.id)) onNotify({id:`friend:${request.id}`,title:'New friend request',body:`${request.friend?.name || request.friend?.address || 'Someone'} wants to connect.`,href:'#/friends/requests',createdAt:Date.now(),kind:'friend'});
    }
    baseline.current={wallet,ids};
  },[wallet,requests,ready,onNotify]);
  return null;
}
