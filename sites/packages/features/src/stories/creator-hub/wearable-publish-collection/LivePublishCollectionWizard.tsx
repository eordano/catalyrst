import { useEffect, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router";
import { formatUnits } from "viem";
import ChPublishCollectionView from "@ui/creatorhub/workflows/ChPublishCollectionView";
import type { PublicationState } from "@ui/generated/catalyst/builder/PublicationState";
import { toErrorMessage } from "@core/lib/errors";
import { track, type TrackContext } from "@core/lib/telemetry/track";
import type { CollectionPublicationFlow, PublicationStage } from "@data/lib/catalyst/builder/collection-publication-flow";
import type { PublicationQuote } from "@data/lib/catalyst/builder/collection-publication";
import type { PublishCollection } from "./machine";
import type { SummaryView } from "./PublishCollectionWizard";

const PROGRESS:Record<PublicationStage,string>={synchronize:"Preparing your Foundation submission",payment:"Approve MANA & sign publish",submit:"Submitting for curation review"};
export default function LivePublishCollectionWizard({collection,summary,flow,trackCtx}:{collection:PublishCollection;summary:SummaryView;flow:CollectionPublicationFlow;trackCtx:TrackContext}) {
  const navigate=useNavigate();
  const [,setSearchParams]=useSearchParams();
  const [view,setView]=useState(collection.items.length?"summary":"blocked");
  const [quote,setQuote]=useState<PublicationQuote>();
  const [publication,setPublication]=useState<PublicationState|null>(null);
  const [email,setEmail]=useState("");
  const [accepted,setAccepted]=useState(false);
  const [error,setError]=useState("");
  const [progress,setProgress]=useState("");
  const [recoveryHash,setRecoveryHash]=useState("");
  const [result,setResult]=useState<Awaited<ReturnType<CollectionPublicationFlow["publish"]>>>();
  const controller=useRef<AbortController|null>(null);
  const retry=useRef<()=>void>(()=>{});
  const feePaid=useRef(false);
  useEffect(()=>()=>controller.current?.abort(),[]);
  useEffect(()=>{
    setSearchParams(prev=>{const params=new URLSearchParams(prev);params.set("step",view);return params;},{replace:true,preventScrollReset:true});
  },[view,setSearchParams]);
  const fee=quote?{lines:quote.lines.map(line=>({...line,manaPerItem:formatUnits(BigInt(line.feeWei),18)})),itemCount:quote.preparation.items.length,manaPerItem:"",totalMana:quote.totalMana}
    :{lines:[],itemCount:collection.items.length,manaPerItem:"",totalMana:""};

  function run(working:string,action:(signal:AbortSignal)=>Promise<void>) {
    if (controller.current) return;
    const ctrl=new AbortController();controller.current=ctrl;
    retry.current=()=>run(working,action);
    setError("");setView(working);
    void action(ctrl.signal).catch(async err=>{
      if (ctrl.signal.aborted) return;
      try { setPublication(await flow.status(collection.id,ctrl.signal)); } catch {}
      if (!ctrl.signal.aborted) {setError(toErrorMessage(err,"Publication could not continue. Retry to resume."));setView("error");}
    }).finally(()=>{if (controller.current===ctrl) controller.current=null;});
  }
  function review() {
    run("checking",async signal=>{
      const reviewed=await flow.review(collection.id,signal);
      signal.throwIfAborted();
      setPublication(reviewed.state);setQuote(reviewed.quote??undefined);
      setView(reviewed.quote?"cost":"resume");
      if (reviewed.quote) track("bd_publish_collection_cost_shown",{mana:Number(reviewed.quote.totalMana)},trackCtx);
    });
  }
  function completed(value:Awaited<ReturnType<CollectionPublicationFlow["publish"]>>) {
    setResult(value);setView("submitted");
    track("bd_publish_submitted",{id:collection.id,itemCount:collection.items.length,mana:quote?Number(quote.totalMana):undefined,stub:false},trackCtx);
  }
  function publish() {
    run("pay",async signal=>{
      const value=await flow.publish({id:collection.id,quote,email,signal,onStage:stage=>{
        if (signal.aborted) return;
        setProgress(PROGRESS[stage]);
        if (stage==="submit" && quote && !feePaid.current) {
          feePaid.current=true;
          track("bd_publish_fee_paid",{mana:Number(quote.totalMana),simulated:false},trackCtx);
        }
      }});
      signal.throwIfAborted();completed(value);
    });
  }
  function recover() {
    run("pay",async signal=>{
      setProgress("Checking your transaction and submission");
      const value=await flow.recover(collection.id,recoveryHash.trim(),signal);
      signal.throwIfAborted();completed(value);
    });
  }
  function cancel() {
    run("checking",async signal=>{
      await flow.cancel(collection.id,signal);signal.throwIfAborted();
      setQuote(undefined);setPublication(null);setView("summary");setAccepted(false);
    });
  }
  return <ChPublishCollectionView live step={view} view={view} collectionName={collection.name} summary={summary} fee={fee}
    email={email} onEmailChange={setEmail} accepted={accepted} onAcceptedChange={setAccepted} error={error}
    progress={progress} txHash={result?.txHash??publication?.tx_hash??""} forumUrl={result?.forumUrl}
    statusHref={`/create/wearables/collections/${encodeURIComponent(collection.id)}`}
    resumeMessage={publication?.status==="published"?"Your collection is already paid for. Continue to finish its Foundation review submission.":publication?.status==="signing"?"A wallet request is unresolved. Check wallet activity and recover its transaction hash below.":"Your publication transaction is pending. Continue to check the same payment."}
    recoveryHash={recoveryHash} onRecoveryHashChange={setRecoveryHash}
    onRecover={publication?.status==="signing"?recover:undefined}
    onCancel={publication?.status==="prepared"?cancel:undefined}
    onNext={()=>{if(view==="summary") {track("bd_publish_collection_started",{id:collection.id,itemCount:collection.items.length},trackCtx);review();} else {setAccepted(false);setView("terms");}}}
    onBack={()=>setView(view==="terms"?"cost":"summary")}
    onAccept={()=>{if(accepted && /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim())) {track("bd_publish_collection_terms_accepted",{},trackCtx);publish();}}}
    onRetry={view==="resume"?(publication?.status==="signing"?undefined:publish):()=>retry.current()}
    onDone={()=>navigate("/create/wearables")}
  />;
}
