import { useEffect, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router";
import ChPublishCollectionView from "@ui/creatorhub/workflows/ChPublishCollectionView";
import type { LinkedPublicationState } from "@ui/generated/catalyst/builder/LinkedPublicationState";
import type { LinkedPublicationFlow, LinkedPublicationReview, LinkedPublicationStage } from "@data/lib/catalyst/builder/linked-publication-flow";
import { toErrorMessage } from "@core/lib/errors";
import { track, type TrackContext } from "@core/lib/telemetry/track";
import type { PublishCollection } from "./machine";
import type { SummaryView } from "./PublishCollectionWizard";

const PROGRESS:Record<LinkedPublicationStage,string>={synchronize:"Preparing your Foundation submission",authorize:"Authorize item slots",submit:"Sending your collection",verify:"Checking Foundation's review records"};

export default function LiveLinkedPublishWizard({collection,summary,flow,trackCtx}:{collection:PublishCollection;summary:SummaryView;flow:LinkedPublicationFlow;trackCtx:TrackContext}) {
  const navigate=useNavigate();
  const [,setSearchParams]=useSearchParams();
  const [view,setView]=useState(collection.items.length?"summary":"blocked");
  const [review,setReview]=useState<LinkedPublicationReview>();
  const [publication,setPublication]=useState<LinkedPublicationState|null>(null);
  const [email,setEmail]=useState("");
  const [accepted,setAccepted]=useState(false);
  const [error,setError]=useState("");
  const [progress,setProgress]=useState("");
  const controller=useRef<AbortController|null>(null);
  const retry=useRef<()=>void>(()=>{});
  useEffect(()=>()=>controller.current?.abort(),[]);
  useEffect(()=>{
    setSearchParams(prev=>{const params=new URLSearchParams(prev);params.set("step",view);return params;},{replace:true,preventScrollReset:true});
  },[view,setSearchParams]);
  function run(working:string,action:(signal:AbortSignal)=>Promise<void>) {
    if(controller.current) return;
    const ctrl=new AbortController();controller.current=ctrl;
    retry.current=()=>run(working,action);setError("");setView(working);
    void action(ctrl.signal).catch(async err=>{
      if(ctrl.signal.aborted) return;
      try { setPublication(await flow.status(collection.id,ctrl.signal)); } catch {}
      if(!ctrl.signal.aborted) { setError(toErrorMessage(err,"Publication could not continue. Retry to resume."));setView("error"); }
    }).finally(()=>{if(controller.current===ctrl)controller.current=null;});
  }
  function loadReview() {
    run("checking",async signal=>{
      const value=await flow.review(collection.id,signal);signal.throwIfAborted();
      setReview(value);setPublication(value.state);
      setView(value.state?.status==="submitted"?"submitted":value.state?.status==="authorized"?"resume":"cost");
    });
  }
  function publish() {
    run("pay",async signal=>{
      const result=await flow.publish({id:collection.id,review,email,signal,onStage:stage=>{if(!signal.aborted)setProgress(PROGRESS[stage]);}});
      signal.throwIfAborted();setPublication(result);setView("submitted");
      track("bd_publish_submitted",{id:collection.id,itemCount:collection.items.length,stub:false},trackCtx);
    });
  }
  function unlock() {
    run("checking",async signal=>{
      await flow.cancel(collection.id,signal);signal.throwIfAborted();
      setReview(undefined);setPublication(null);setAccepted(false);setView("summary");
    });
  }
  return <ChPublishCollectionView live step={view} view={view} collectionName={collection.name} summary={summary}
    linked={{providerName:review?.providerName??"your linked provider",availableSlots:review?.availableSlots??null,requiredSlots:review?.preparation.item_ids.length??collection.items.length}}
    email={email} onEmailChange={setEmail} accepted={accepted} onAcceptedChange={setAccepted} error={error} progress={progress}
    forumUrl={publication?.forum_url??undefined}
    statusHref={publication?.status==="submitted"?`https://decentraland.org/builder/item-editor?collection=${encodeURIComponent(collection.id)}`:undefined}
    resumeMessage="Your item-slot authorization is saved. Continue to check Foundation and finish the same submission."
    onNext={()=>view==="summary"?loadReview():setView("terms")}
    onBack={()=>view==="terms"?setView("cost"):setView("summary")} onAccept={publish}
    onRetry={()=>view==="resume"?publish():retry.current()}
    onCancel={publication?.status==="prepared"?unlock:undefined}
    onDone={()=>navigate("/create/wearables")}/>
}
