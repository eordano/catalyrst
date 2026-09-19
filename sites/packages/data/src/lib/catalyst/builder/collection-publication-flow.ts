import { foundationFetch } from "./foundation-fetch";
import type { Eip1193Provider } from "../../auth/wallet";
import { collectionPublishWallet } from "./collection-publish-wallet";
import { foundationPublication } from "./foundation-publication";
import { fetchPublicationPreparation, type PublicationQuote } from "./collection-publication";

type Session = {sign:(method:string,path:string)=>Promise<{headers:Record<string,string>}>;address:string;provider:Eip1193Provider;fetch:(url:string,init?:RequestInit)=>Promise<Response>};
export type PublicationStage = "synchronize" | "payment" | "submit";
export function collectionPublicationFlow(getSession:()=>Session) {
  const wallet=collectionPublishWallet(getSession);
  const foundation=foundationPublication(()=>({base:"",fetch:foundationFetch(getSession)}));
  const preparation=(id:string,signal?:AbortSignal)=>fetchPublicationPreparation(id,{base:"",signal,fetchImpl:(input,init)=>getSession().fetch(String(input),init)});
  async function submit(id:string, result:Awaited<ReturnType<typeof wallet.publish>>, signal?:AbortSignal) {
    const p=await preparation(id,signal);
    return {...result,...await foundation.submit(p,result.contractAddress,signal)};
  }
  return {
    status:wallet.status,
    cancel:wallet.cancel,
    async review(id:string,signal?:AbortSignal) {
      const state=await wallet.status(id,signal);
      if (state && ["signing","submitted","published"].includes(state.status)) return {state,quote:null};
      if (state?.status!=="prepared") await foundation.inspect(id,signal);
      return {state,quote:await wallet.quote({id,signal})};
    },
    async publish({id,quote,email,signal,onStage}:{id:string;quote?:PublicationQuote;email:string;signal?:AbortSignal;onStage?:(stage:PublicationStage)=>void}) {
      onStage?.("payment");
      const result=await wallet.publish({id,quote,signal,prepare:async(q,signal)=>{
        onStage?.("synchronize");
        await foundation.synchronize(q,email,getSession().provider,signal);
        onStage?.("payment");
      }});
      onStage?.("submit");
      return submit(id,result,signal);
    },
    async recover(id:string,txHash:string,signal?:AbortSignal) {
      return submit(id,await wallet.recover({id,txHash,signal}),signal);
    },
  };
}
export type CollectionPublicationFlow = ReturnType<typeof collectionPublicationFlow>;
