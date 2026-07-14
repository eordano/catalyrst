import { useEffect, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { Link, useSearchParams } from "react-router";

import CreateCollectionView from "@ui/creatorhub/workflows/CreateCollectionView";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  ACCEPTED_FILE_EXTENSIONS,
  createCollectionMachine,
  parseCollectionType,
  resolveCreateSnapshot,
  slugToState,
  stateToSlug,
  NAME_MAX,
  publishCost,
  type CollectionType,
  type DraftItem,
  type MintFn,
  type TrackFn,
} from "./machine";

export type WizardOptions = {
  feePerItem: number;
  nameSuggestions: string[];
};

export type CreateCollectionWizardProps = {
  trackCtx: TrackContext;
  options: WizardOptions;
  initialStep?: string;
  initialType?: string;
  mint?: MintFn;
  track?: TrackFn;
};

function fileExtension(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot === -1 ? "" : name.slice(dot).toLowerCase();
}

let uploadSeq = 0;
function uploadId(): string {
  uploadSeq += 1;
  return `upload-${Date.now().toString(36)}-${uploadSeq}`;
}

export default function CreateCollectionWizard({
  trackCtx,
  options,
  initialStep,
  initialType,
  mint,
  track,
}: CreateCollectionWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const urlName = searchParams.get("name")?.trim() || undefined;
  const raw = slugToState(urlStep);
  let stateId = raw === "submitting" ? "reviewing" : raw;
  if (stateId === "reviewing" || stateId === "done" || stateId === "error") {
    stateId = "editingItems";
  }
  if (stateId !== "naming" && !urlName) stateId = "naming";
  const urlType = (searchParams.get("type")?.trim() || initialType) ?? undefined;
  const collectionType = parseCollectionType(urlType);

  return (
    <CreateCollectionWizardInner
      key={collectionType}
      stateId={stateId}
      collectionType={collectionType}
      committedName={urlName}
      trackCtx={trackCtx}
      options={options}
      mint={mint}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  collectionType: CollectionType;
  committedName?: string;
  trackCtx: TrackContext;
  options: WizardOptions;
  mint?: MintFn;
  track?: TrackFn;
};

function CreateCollectionWizardInner({
  stateId,
  collectionType,
  committedName,
  trackCtx,
  options,
  mint,
  track,
}: InnerProps) {
  const [searchParams, setSearchParams] = useSearchParams();
  const [uploads, setUploads] = useState<DraftItem[]>([]);
  const [skippedFiles, setSkippedFiles] = useState<string[]>([]);

  const snapshot = useRef(
    resolveCreateSnapshot({
      step: stateId,
      trackCtx,
      feePerItem: options.feePerItem,
      mint,
      track,
      seed: { name: committedName, type: collectionType },
    }),
  ).current;

  const [state, send, actorRef] = useMachine(createCollectionMachine, {
    input: { trackCtx, type: collectionType, feePerItem: options.feePerItem, mint, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const urlStep = searchParams.get("step")?.trim() || null;

  const latestName = useRef(state.context.name);
  latestName.current = state.context.name;

  const statusRef = useRef(state.status);
  statusRef.current = state.status;

  const syncedStep = useRef<string | null>(null);
  useEffect(() => {
    const writeUrl = (slug: string, replace: boolean) => {
      syncedStep.current = slug;
      setSearchParams(
        (prev) => {
          const params = new URLSearchParams(prev);
          params.set("step", slug);
          const name = latestName.current.trim();
          if (name) params.set("name", name);
          return params;
        },
        { replace, preventScrollReset: true },
      );
    };

    if (step !== syncedStep.current) {
      const transient = step === "submit" || step === "done" || step === "error";
      writeUrl(step, syncedStep.current === null || transient);
      return;
    }

    if (urlStep !== syncedStep.current && urlStep) {
      const target = slugToState(urlStep);
      if (target !== value && statusRef.current === "active") {
        send({ type: "GOTO", step: target });
      }
      const landed = stateToSlug(actorRef.getSnapshot().value as string);
      if (landed === urlStep) {
        syncedStep.current = urlStep;
      } else {
        writeUrl(landed, true);
      }
    }
  }, [step, urlStep, value, send, actorRef, setSearchParams]);

  const uploadsRef = useRef(uploads);
  uploadsRef.current = uploads;
  useEffect(
    () => () => {
      for (const u of uploadsRef.current) {
        if (u.thumbnail) URL.revokeObjectURL(u.thumbnail);
      }
    },
    [],
  );

  const addFiles = (files: File[]) => {
    const accepted: DraftItem[] = [];
    const skipped: string[] = [];
    for (const file of files) {
      const ext = fileExtension(file.name);
      if (!(ACCEPTED_FILE_EXTENSIONS as readonly string[]).includes(ext)) {
        skipped.push(file.name);
        continue;
      }
      accepted.push({
        id: uploadId(),
        name: file.name,
        size: file.size,
        fileType: ext.slice(1),
        thumbnail: ext === ".png" ? URL.createObjectURL(file) : undefined,
      });
    }
    if (accepted.length) setUploads((prev) => [...prev, ...accepted]);
    setSkippedFiles(skipped);
  };

  const removeUpload = (id: string) => {
    setUploads((prev) => {
      const target = prev.find((u) => u.id === id);
      if (target?.thumbnail) URL.revokeObjectURL(target.thumbnail);
      return prev.filter((u) => u.id !== id);
    });
  };

  const items = state.context.items;
  const cost = publishCost(state.context.type, items.length, options.feePerItem);

  return (
    <CreateCollectionView
      view={value}
      step={step}
      name={state.context.name}
      type={state.context.type}
      items={items}
      uploads={uploads}
      skippedFiles={skippedFiles}
      accept={[...ACCEPTED_FILE_EXTENSIONS]}
      nameSuggestions={options.nameSuggestions}
      feePerItem={options.feePerItem}
      nameMax={NAME_MAX}
      cost={cost}
      error={state.context.error}
      collectionHref={`/create/wearables/collections/${state.context.result?.collectionId ?? ""}`}
      thirdPartyHref="?type=linked"
      standardHref="?type=standard"
      LinkComponent={Link}
      onSubmitName={(name: string) => send({ type: "SUBMIT_NAME", name })}
      onFilesSelected={addFiles}
      onRemoveUpload={removeUpload}
      onAddItems={() => send({ type: "ADD_ITEMS", items: uploads })}
      onSubmit={() => send({ type: "SUBMIT" })}
      onBack={() => send({ type: "BACK" })}
      onRetry={() => send({ type: "RETRY" })}
    />
  );
}
