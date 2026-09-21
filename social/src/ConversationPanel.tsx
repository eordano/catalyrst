import { useEffect, useState } from 'react';
import { api,execute,type Community,type Message,type Opened,type WalletIdentity } from './api';
import { Avatar,Name } from './Profile';
import { MemberManager } from './MemberManager';
import { Icon } from './Icon';
import { MemberPresence, useMemberLocations, memberIsOnline } from './MemberPresence';
export type Panel = 'reports' | 'members' | 'bans' | 'search' | 'pins' | 'thread';
type Member = {address:string;memberAddress?:string;role:string};
type Reply = NonNullable<Message['replies']>[number];
const merge = <T extends {id:string;seq:number}>(old:T[],next:T[]) => [...new Map([...old,...next].map(m=>[m.id,m])).values()].sort((a,b)=>a.seq-b.seq);
export function ConversationPanel({mode,community,identity,opened,parent,page=false,onClose,onThread,onReply,onRefresh,memberStatuses={}}: {mode:Panel;community:Community;identity:WalletIdentity;opened:Opened;parent?:Message;page?:boolean;onClose:()=>void;onThread:(message:Message)=>void;onReply:(text:string)=>Promise<void>;onRefresh:()=>void;memberStatuses?:Record<string,number>}) {
  const [reports,setReports]=useState<{id:string;wallet:string;text:string;channel:string;reports:number}[]>([]);
  const [managing,setManaging]=useState<Member|null>(null);
  const [search,setSearch]=useState(''),[error,setError]=useState('');
  const [members,setMembers]=useState<Member[]>([]),[total,setTotal]=useState(0),[offset,setOffset]=useState(0);
  const [results,setResults]=useState<Message[]>([]),[more,setMore]=useState(false),[before,setBefore]=useState<number>();
  const [replies,setReplies]=useState<Reply[]>(parent?.replies||[]),[threadParent,setThreadParent]=useState(parent);
  const [replyMore,setReplyMore]=useState(parent?.hasMoreReplies||false),[replyBefore,setReplyBefore]=useState<number>();
  const [loading,setLoading]=useState(false),[retry,setRetry]=useState(0),[draft,setDraft]=useState(''),[sending,setSending]=useState(false);
  const memberLocations = useMemberLocations(members.map(member => member.address), mode === 'members');
  const sortedMembers = [...members].sort((a, b) => Number(memberIsOnline(memberLocations[b.address.toLowerCase()], memberStatuses[b.address.toLowerCase()])) - Number(memberIsOnline(memberLocations[a.address.toLowerCase()], memberStatuses[a.address.toLowerCase()])));
  const endpoint=`/communities/${community.id}/messages`;
  useEffect(()=>{
    let current=true; const abort=new AbortController(); let timer:ReturnType<typeof setTimeout>;
    async function load(){
      setLoading(true);setError('');
      try {
        if(mode==='reports'){const r=await execute<{reports:typeof reports}>(identity,{type:'community_reports',community_id:community.id},()=>current);if(current)setReports(r.reports);}
        else if(mode==='members'||mode==='bans'){
          const r=await execute<{data:{results:Member[];total:number}}>(identity,mode==='members'?{type:'community_members',community_id:community.id,offset}:{type:'manage_community',community_id:community.id,action:{kind:'bans',offset}},()=>current);
          if(current){setMembers(all=>[...new Map([...(offset?all:[]),...r.data.results.map(m=>({...m,address:m.address||m.memberAddress||""})).filter(m=>/^0x[0-9a-f]{40}$/i.test(m.address))].map(m=>[m.address,m])).values()]);setTotal(r.data.total);}
        }else{
          const q=new URLSearchParams();
          if(mode==='thread'&&parent){q.set('thread',parent.id);if(replyBefore)q.set('reply_before',String(replyBefore));}
          else{if(mode==='search')q.set('search',search);if(mode==='pins')q.set('pinned','true');if(before)q.set('before',String(before));}
          const r=await api<{messages:Message[];replies:Reply[];parent:Message;hasMore:boolean}>(`${endpoint}?${q}`,{signal:abort.signal,headers:{Authorization:`Bearer ${opened.readToken}`}});
          if(current){if(mode==='thread'){setReplies(all=>merge(all,r.replies));setThreadParent(r.parent);setReplyMore(r.hasMore);}else{setResults(all=>before?merge(all,r.messages):r.messages);setMore(r.hasMore);}}
        }
      }catch(e){if(current)setError(e instanceof Error?e.message:'Could not load this panel.');}finally{if(current)setLoading(false);}
    }
    timer=setTimeout(()=>void load(),mode==='search'?200:0);
    return()=>{current=false;abort.abort();clearTimeout(timer);};
  },[mode,community.id,identity,opened.readToken,parent?.id,search,before,replyBefore,offset,retry]);
  useEffect(()=>{if(mode!=='thread')return;const timer=setInterval(()=>setRetry(n=>n+1),4000);return()=>clearInterval(timer);},[mode]);
  // Polling always refreshes the newest replies, without losing older pages.
  useEffect(()=>{if(mode!=='thread'||!parent||!replyBefore)return;let active=true;const abort=new AbortController();api<{replies:Reply[];parent:Message}>(`${endpoint}?thread=${parent.id}`,{signal:abort.signal,headers:{Authorization:`Bearer ${opened.readToken}`}}).then(r=>{if(active){setReplies(all=>merge(all,r.replies));setThreadParent(r.parent);}}).catch(()=>{});return()=>{active=false;abort.abort();};},[retry,replyBefore,opened.readToken]);
  async function unban(address:string){if(loading)return;setLoading(true);setError('');try{await execute(identity,{type:'manage_community',community_id:community.id,action:{kind:'unban',address}});setOffset(0);setRetry(n=>n+1);}catch(e){setError(e instanceof Error?e.message:'Could not unban this member.');}finally{setLoading(false);}}
  async function review(message_id:string,action:'dismiss'|'remove'){if(loading)return;setLoading(true);setError('');try{await execute(identity,{type:'message_action',community_id:community.id,message_id,action});setRetry(n=>n+1);onRefresh();}catch(e){setError(e instanceof Error?e.message:'Could not review report.');}finally{setLoading(false);}}
  const title=mode==='reports'?'Reported messages':mode==='thread'?'Replies':mode==='members'?'Members':mode==='bans'?'Banned members':mode==='pins'?'Pinned messages':'Search';
  return <aside className={page?'conversation-panel page':'conversation-panel'} aria-label={title}><header><div>{page?<h1>{title}</h1>:<h2>{title}</h2>}<small>{page&&mode==='members'?`${community.membersCount.toLocaleString()} in ${community.name}`:community.name}</small></div>{!page&&<button aria-label="Close panel" onClick={onClose}><Icon name="close"/></button>}</header>
    {mode==='search'&&<input autoFocus aria-label="Search messages" placeholder="Search this conversation" value={search} onChange={e=>{setSearch(e.target.value);setBefore(undefined);}}/>}
    <div className="panel-scroll">
      {mode==='reports'?<>{reports.map(r=><article className="reported-message" key={r.id}><strong><Name wallet={r.wallet}/></strong><small>#{r.channel} &#xb7; {r.reports} reports</small><p>{r.text}</p><div className="call-controls"><button className="outline-button" disabled={loading} onClick={()=>void review(r.id,'dismiss')}>Dismiss report</button><button className="primary" disabled={loading} onClick={()=>void review(r.id,'remove')}>Remove message</button></div></article>)}{!loading&&!reports.length&&!error&&<p className="panel-empty">No reported messages.</p>}{reports.length===100&&<p className="muted">Review these reports to see the next batch.</p>}</>:mode==='members'||mode==='bans'?<>{sortedMembers.map(m=><div className="member-row" key={m.address}><Avatar wallet={m.address}/><div><strong><Name wallet={m.address}/></strong><small>{mode==='bans'?'Banned':m.role}</small>{mode==='members'&&<MemberPresence location={memberLocations[m.address.toLowerCase()]} status={memberStatuses[m.address.toLowerCase()]}/>}</div>{mode==='bans'?<button className="outline-button" disabled={loading} onClick={()=>void unban(m.address)}>Unban</button>:["owner","moderator"].includes(community.role||'')&&m.role!=='owner'&&m.address.toLowerCase()!==identity.address.toLowerCase()&&<button aria-label={`Manage member ${m.address}`} onClick={()=>setManaging(m)}><Icon name="settings"/></button>}</div>)}{members.length<total&&!loading&&<button className="outline-button" onClick={()=>setOffset(members.length)}>Load more members</button>}{!members.length&&!loading&&!error&&<p className="panel-empty">{mode==='bans'?'No banned members.':'No members found.'}</p>}</>:mode==='thread'?<>{threadParent&&<><article className="panel-message"><Avatar wallet={threadParent.wallet}/><div><strong><Name wallet={threadParent.wallet}/></strong><p>{threadParent.text||'Shared a scene'}</p></div></article><div className="reply-divider">{threadParent.replyCount??replies.length} replies</div>{replyMore&&<button disabled={loading} className="outline-button" onClick={()=>setReplyBefore(replies[0]?.seq)}>Load older replies</button>}{replies.map(r=><article className="panel-message" key={r.id}><Avatar wallet={r.wallet}/><div><strong><Name wallet={r.wallet}/></strong><time>{new Date(r.createdAt).toLocaleTimeString([],{hour:'2-digit',minute:'2-digit'})}</time><p>{r.text}</p></div></article>)}</>}</>:<>{results.map(m=><button className="search-result" key={m.id} onClick={()=>onThread(m)}><strong><Name wallet={m.wallet}/></strong><small>{new Date(m.createdAt).toLocaleDateString()}</small><p>{m.text||'Shared a scene'}</p></button>)}{!results.length&&!loading&&!error&&<p className="panel-empty">{mode==='pins'?'No pinned messages yet.':'No messages found.'}</p>}{more&&<button className="outline-button" disabled={loading} onClick={()=>setBefore(results[0]?.seq)}>Load more results</button>}</>}
      {loading&&<p role="status" className="muted">Loading&#x2026;</p>}{error&&<div role="alert"><p>{error}</p><button onClick={()=>setRetry(n=>n+1)}>Try again</button><button onClick={onRefresh}>Refresh access</button></div>}
    </div>
    {mode==='thread'&&parent&&<form className="reply-composer" onSubmit={async e=>{e.preventDefault();if(sending||!draft.trim())return;setSending(true);setError('');try{await onReply(draft);setDraft('');setRetry(n=>n+1);}catch(e){setError(e instanceof Error?e.message:'Reply could not be sent.');}finally{setSending(false);}}}><textarea autoFocus aria-label="Reply" placeholder="Reply&#x2026;" maxLength={4000} value={draft} onChange={e=>setDraft(e.target.value)} onKeyDown={e=>{if(e.key==='Enter'&&!e.shiftKey&&!e.nativeEvent.isComposing){e.preventDefault();e.currentTarget.form?.requestSubmit();}}}/><button className="primary" aria-label="Send reply" disabled={sending||!draft.trim()}><Icon name="send"/></button></form>}
    {managing&&<MemberManager community={community} identity={identity} member={managing} onClose={()=>setManaging(null)} onSaved={()=>{setOffset(0);setRetry(n=>n+1);}}/>}
  </aside>;
}
