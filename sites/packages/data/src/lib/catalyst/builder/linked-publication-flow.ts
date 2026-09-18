import type { LinkedPublicationPreparation } from "@ui/generated/catalyst/builder/LinkedPublicationPreparation";
import type { LinkedPublicationState } from "@ui/generated/catalyst/builder/LinkedPublicationState";
import type { Eip1193Provider } from "../../auth/wallet";
import { LinkedPublicationPreparationSchema, LinkedPublicationStateSchema } from "../generated-schemas/builder";
import { draftRequest } from "./drafts";
import { foundationFetch } from "./foundation-fetch";
import { foundationPublication } from "./foundation-publication";
import { linkedPublishWallet } from "./linked-publish-wallet";
import { linkedProviderSlots, listLinkedProviders } from "./linked-providers";

type Session = { address:string; provider:Eip1193Provider; fetch:(url:string,init?:RequestInit)=>Promise<Response>; sign:(method:string,path:string)=>Promise<{headers:Record<string,string>}> };
export type LinkedPublicationStage = "synchronize" | "authorize" | "submit" | "verify";
export type LinkedPublicationReview = { preparation:LinkedPublicationPreparation; state:LinkedPublicationState|null; availableSlots:number|null; providerName:string };

export function linkedPublicationFlow(getSession:()=>Session) {
  function scope(signal?:AbortSignal) {
    const owner=getSession().address;
    const current=()=>{
      signal?.throwIfAborted();
      const session=getSession();
      if(session.address.toLowerCase()!==owner.toLowerCase()) throw new Error("Your account changed. Sign in with the collection owner and retry.");
      return session;
    };
    const fetch=foundationFetch(current);
    const request=(id:string,method:string,body?:unknown,suffix="")=>draftRequest(`/v1/collections/${encodeURIComponent(id)}/linked-publication${suffix}`,
      {base:"",signal,fetch:(url,init)=>current().fetch(url,init)},
      {method,headers:body===undefined?undefined:{"content-type":"application/json"},body:body===undefined?undefined:JSON.stringify(body)});
    const state=async(id:string)=>LinkedPublicationStateSchema.nullable().parse(await request(id,"GET",undefined,"/status"));
    const verify=async(id:string)=>{
      const path=`/v1/collections/${encodeURIComponent(id)}`;
      const [collection,items,curations]=await Promise.all([path,`${path}/items`,`${path}/itemCurations`].map(path=>current().sign("GET",path)));
      return LinkedPublicationStateSchema.parse(await request(id,"POST",{collection:collection.headers,items:items.headers,curations:curations.headers},"/verify"));
    };
    return {owner,current,fetch,request,state,verify,foundation:foundationPublication(()=>({base:"",fetch})),
      authorize:linkedPublishWallet(()=>({...current(),fetch}))};
  }
  return {
    status(id:string,signal?:AbortSignal) { return scope(signal).state(id); },
    async cancel(id:string,signal?:AbortSignal) { await scope(signal).request(id,"DELETE"); },
    async review(id:string,signal?:AbortSignal):Promise<LinkedPublicationReview> {
      const s=scope(signal),state=await s.state(id);
      if(!state) await s.foundation.inspect(id,signal);
      const preparation=state?.preparation??LinkedPublicationPreparationSchema.parse(await s.request(id,"GET"));
      if(preparation.creator.toLowerCase()!==s.owner.toLowerCase()) throw new Error("Sign in with the collection owner to publish.");
      if(state && ["authorized","submitted"].includes(state.status)) return {preparation,state,availableSlots:null,providerName:preparation.third_party_id.split(":").at(-1)??"Linked provider"};
      const opts={fetch:s.fetch,signal};
      const [providers,availableSlots]=await Promise.all([listLinkedProviders(s.owner,opts),linkedProviderSlots(preparation.third_party_id,opts)]);
      const provider=providers.find(provider=>provider.id===preparation.third_party_id);
      if(!provider) throw new Error("You no longer manage this linked provider. Ask its owner to restore your access.");
      return {preparation,state,availableSlots,providerName:provider.name};
    },
    async publish({id,review,email,signal,onStage}:{id:string;review?:LinkedPublicationReview;email:string;signal?:AbortSignal;onStage?:(stage:LinkedPublicationStage)=>void}) {
      const s=scope(signal);
      let state=await s.state(id);
      if(!state) {
        if(!review || review.preparation.id!==id || review.preparation.creator.toLowerCase()!==s.owner.toLowerCase()) throw new Error("Review this linked collection before authorizing its item slots.");
        state=LinkedPublicationStateSchema.parse(await s.request(id,"POST",{revision:review.preparation.revision}));
      }
      if(state.status==="submitted") return state;
      if(state.status!=="authorized") {
        onStage?.("synchronize");
        await s.foundation.synchronizeLinked(state.preparation,email,signal);
        state=LinkedPublicationStateSchema.parse(await s.request(id,"PATCH",{revision:state.preparation.revision}));
        if(state.status==="submitted") return state;
        if(!state.cheque) {
          onStage?.("authorize");
          const cheque=await s.authorize({thirdPartyId:state.preparation.third_party_id,qty:state.preparation.item_ids.length,salt:state.salt},signal);
          state=LinkedPublicationStateSchema.parse(await s.request(id,"PUT",cheque));
        }
      }
      onStage?.("verify");
      state=await s.verify(id);
      if(state.status==="submitted") return state;
      if(state.status!=="authorized" || !state.cheque) throw new Error("The saved slot authorization is unavailable. Reload to resume this publication.");
      onStage?.("submit");
      let submissionError:unknown;
      try { await s.foundation.submitLinked(state.preparation,state.cheque,signal); }
      catch(error) { submissionError=error; }
      onStage?.("verify");
      const verified=await s.verify(id);
      if(verified.status==="submitted") return verified;
      if(submissionError) throw submissionError;
      throw new Error("Foundation has not confirmed this submission yet. Retry to check the same authorization.");
    },
  };
}
export type LinkedPublicationFlow = ReturnType<typeof linkedPublicationFlow>;
