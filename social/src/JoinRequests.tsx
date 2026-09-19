import { useEffect,useState,useRef } from 'react';
import { execute, type Community, type WalletIdentity } from './api';
import { Avatar,Name } from './Profile';
import { Dialog } from './Dialog';
type Request = {id:string;memberAddress:string;status:string};
export function JoinRequests({community,identity,onClose}: {community:Community;identity:WalletIdentity;onClose:()=>void}) {
  const active = useRef(true);
  useEffect(() => {active.current=true;return () => {active.current=false;};},[]);
  const [requests,setRequests] = useState<Request[]>([]);
  const [total,setTotal] = useState(0);
  const [offset,setOffset] = useState(0);
  const [version,setVersion] = useState(0);
  const [error,setError] = useState('');
  const [busy,setBusy] = useState(false);
  useEffect(() => { let active=true; setBusy(true); setError(''); execute<{data:{results:Request[];total:number}}>(identity,{type:'community_requests',community_id:community.id,offset},()=>active).then(r=>{if(active){setRequests(all=>offset?[...all,...r.data.results]:r.data.results);setTotal(r.data.total);}}).catch(e=>{if(active)setError(e.message);}).finally(()=>{if(active)setBusy(false);});return()=>{active=false;}; },[community.id,identity,offset,version]);
  async function resolve(request:Request,accept:boolean) { if(busy)return;setBusy(true);setError('');try{await execute(identity,{type:'resolve_request',community_id:community.id,request_id:request.id,accept},()=>active.current);if(!active.current)return;setOffset(0);setVersion(v=>v+1);}catch(e){setError(e instanceof Error?e.message:'Could not update request.');}finally{setBusy(false);} }
  return <Dialog title="Join requests" onClose={onClose}><div className="dialog-content"><h2>Welcome them in.</h2><p>{community.name}</p>{requests.filter(r=>r.status==='pending').map(r=><div className="join-request" key={r.id}><Avatar wallet={r.memberAddress} /><strong><Name wallet={r.memberAddress} /></strong><button className="primary" disabled={busy} onClick={()=>void resolve(r,true)}>Accept</button><button disabled={busy} onClick={()=>void resolve(r,false)}>Decline</button></div>)}{busy&&<p role="status">Updating requests&#x2026;</p>}{!busy&&!error&&!requests.some(r=>r.status==='pending')&&<p>You're all caught up.</p>}{requests.length<total&&!busy&&<button className="outline-button" onClick={()=>setOffset(requests.length)}>Load more requests</button>}{error&&<div role="alert"><p>{error}</p><button onClick={()=>{setOffset(0);setVersion(v=>v+1);}}>Refresh requests</button></div>}</div></Dialog>;
}
