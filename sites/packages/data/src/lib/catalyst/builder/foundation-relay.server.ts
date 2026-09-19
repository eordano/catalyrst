import { linkedProviderId } from "./linked-providers";
import { verifyAuthChainRequest } from "../creator-hub/scene-drafts.server";

const BASE="https://builder-api.decentraland.org/v1";
const UUID="[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}";
const allowed:Record<string,RegExp>={
  GET:new RegExp(`^/(?:thirdParties|collections/${UUID}(?:/items)?|items/${UUID})$`,"i"),
  PUT:new RegExp(`^/(?:collections|items)/${UUID}$`,"i"),
  POST:new RegExp(`^/(?:collections/${UUID}/(?:tos|publish|post)|items/${UUID}/files)$`,"i"),
};
function providerSlots(path: string): boolean {
  const match = /^\/thirdParties\/([^/]{1,768})\/slots$/.exec(path);
  if (!match) return false;
  try {
    const id = decodeURIComponent(match[1]);
    return encodeURIComponent(id) === match[1] && linkedProviderId.safeParse(id).success;
  } catch { return false; }
}
const MAX_BODY=22*1024*1024;
const privateHeaders={"cache-control":"private, no-store"};
export async function relayFoundation(request:Request,path:string,fetchImpl:typeof fetch=fetch) {
  const error=(message:string,status:number)=>Response.json({error:message},{status,headers:privateHeaders});
  if (!(allowed[request.method]?.test(path) || (request.method === "GET" && providerSlots(path))) || new URL(request.url).search) return error("Unknown Foundation operation.",404);
  const url=BASE+path;
  const auth=await verifyAuthChainRequest(new Request(url,{method:request.method,headers:request.headers}));
  if (!auth.ok) return error(auth.error,auth.status);
  if (Number(request.headers.get("content-length")??0)>MAX_BODY) return error("Publication upload exceeds 22 MB.",413);
  const headers=new Headers();
  for (const [key,value] of request.headers) {
    if (/^x-identity-(auth-chain-\d+|timestamp|metadata)$/.test(key) || key==="content-type") headers.set(key,value);
  }
  try {
    let body:Uint8Array<ArrayBuffer>|undefined;
    if (request.body) {
      const reader=request.body.getReader(),chunks:Uint8Array[]=[];let size=0;
      for (;;) {
        const {done,value}=await reader.read();if(done)break;
        size+=value.length;if(size>MAX_BODY){await reader.cancel();return error("Publication upload exceeds 22 MB.",413);}chunks.push(value);
      }
      body=new Uint8Array(size);let offset=0;
      for(const chunk of chunks){body.set(chunk,offset);offset+=chunk.length;}
    }
    const response=await fetchImpl(url,{method:request.method,headers,body,redirect:"error",signal:AbortSignal.any([request.signal,AbortSignal.timeout(30000)])});
    return new Response(response.body,{status:response.status,headers:{...privateHeaders,"content-type":response.headers.get("content-type")??"application/json","x-content-type-options":"nosniff"}});
  } catch {
    return error("Foundation Builder is unavailable. Retry to continue your submission.",503);
  }
}
