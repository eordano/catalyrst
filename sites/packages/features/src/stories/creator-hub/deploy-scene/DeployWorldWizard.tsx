import { useEffect, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { useNavigate, useSearchParams } from "react-router";

import DeployWorldView from "@ui/creatorhub/workflows/DeployWorldView";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  exceedsQuota,
  formatSize,
  landJumpUrl,
  totalBytes,
  worldJumpUrl,
  type DeployFile,
  type DeployName,
  type DeployWorldData,
} from "@data/lib/catalyst/creator-hub/deploy-world";
import {
  deployWorldMachine,
  resolveDeploySnapshot,
  slugToState,
  stateToSlug,
  type DeployFn,
  type DeployLand,
  type TrackFn,
} from "./machine";

export function claimNameUrl(searchParams: URLSearchParams): string {
  const params = new URLSearchParams();
  params.set("from", "deploy-world");
  const project = searchParams.get("project")?.trim();
  if (project) params.set("project", project);
  const world = searchParams.get("world")?.trim();
  if (world) params.set("world", world);
  const origin = searchParams.get("from")?.trim();
  if (origin) params.set("origin", origin);
  return `/marketplace/claim-name?${params.toString()}`;
}

export type DeployWorldWizardProps = {
  trackCtx: TrackContext;
  names: DeployName[];
  namesEmpty: boolean;
  files: DeployFile[];
  maxFileSizeMb: number;
  project: DeployWorldData["project"];
  owner: DeployWorldData["owner"];
  claimNote?: string;
  land?: DeployLand | null;
  landNotice?: string;
  runtimeNote?: string;
  filesReady?: boolean;
  prepareReview?: () => Promise<boolean>;
  initialStep?: string;
  closeTo?: string;
  deploy?: DeployFn;
  track?: TrackFn;
};

export default function DeployWorldWizard(props: DeployWorldWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  let stateId = slugToState(urlStep);
  if (stateId === "review" && !props.filesReady) stateId = "selectWorld";
  if (stateId === "namesEmpty" && !props.namesEmpty) stateId = "destination";
  if ((stateId === "selectWorld" || stateId === "review") && props.namesEmpty)
    stateId = "destination";

  const rawUrlName = searchParams.get("name")?.trim() || undefined;
  const urlName = rawUrlName?.toLowerCase();
  const ownedUrlName = urlName
    ? props.names.find((n) => n.name.toLowerCase() === urlName)?.name
    : undefined;

  const pendingName =
    searchParams.get("claimed") === "1" && rawUrlName && !ownedUrlName
      ? rawUrlName
      : undefined;

  return (
    <DeployWorldWizardInner
      stateId={stateId}
      preferredName={ownedUrlName}
      pendingName={pendingName}
      {...props}
    />
  );
}

type InnerProps = DeployWorldWizardProps & {
  stateId: ReturnType<typeof slugToState>;
  preferredName?: string;
  pendingName?: string;
};

function DeployWorldWizardInner({
  stateId,
  preferredName,
  pendingName,
  trackCtx,
  names,
  namesEmpty,
  files,
  filesReady,
  prepareReview,
  maxFileSizeMb,
  project,
  owner,
  claimNote,
  land,
  landNotice,
  runtimeNote,
  closeTo,
  deploy,
  track,
}: InnerProps) {
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useNavigate();
  const urlTarget =
    searchParams.get("target") === "land" && land
      ? ("land" as const)
      : ("world" as const);

  const refreshNames = () => {
    if (typeof window === "undefined") return;
    const params = new URLSearchParams(window.location.search);
    params.delete("step");
    const qs = params.toString();
    window.location.assign(window.location.pathname + (qs ? `?${qs}` : ""));
  };

  const close = () => {
    const idx =
      typeof window !== "undefined"
        ? Number((window.history.state as { idx?: number } | null)?.idx ?? 0)
        : 0;
    if (idx > 0) navigate(-1);
    else navigate(closeTo ?? "/create/scenes", { replace: true });
  };

  const defaultName = preferredName ?? names[0]?.name;

  const deployRef = useRef(deploy);
  deployRef.current = deploy;
  const liveDeploy = useRef<DeployFn | undefined>(
    deploy ? (args) => (deployRef.current ?? deploy)(args) : undefined,
  ).current;

  const snapshot = useRef(
    resolveDeploySnapshot({
      step: stateId,
      trackCtx,
      files,
      maxFileSizeMb,
      namesEmpty,
      land,
      target: urlTarget,
      deploy: liveDeploy,
      track,
      name: defaultName,
    }),
  ).current;

  const [state, send] = useMachine(deployWorldMachine, {
    input: {
      trackCtx,
      files,
      maxFileSizeMb,
      namesEmpty,
      defaultName,
      land,
      deploy: liveDeploy,
      track,
    },
    snapshot,
  });

  useEffect(() => {
    send({ type: "SET_FILES", files });
  }, [files, send]);

  const [pickedName, setPickedName] = useState<string | undefined>(undefined);

  const goReview = async (name: string) => {
    setPickedName(name);
    if (!filesReady && prepareReview) {
      let ok = false;
      try {
        ok = await prepareReview();
      } catch {
        ok = false;
      }
      if (!ok) return;
    }
    send({ type: "PICK_NAME", name });
  };

  const chooseLand = async () => {
    if (!filesReady && prepareReview) {
      let ok = false;
      try {
        ok = await prepareReview();
      } catch {
        ok = false;
      }
      if (!ok) return;
    }
    send({ type: "CHOOSE_LAND" });
  };

  const value = state.value as string;
  const step = stateToSlug(value);
  const target = state.context.target;
  const contextName = state.context.name;

  const lastStep = useRef<string | null>(null);
  useEffect(() => {
    const key = `${step}:${target}`;
    if (lastStep.current === key) return;
    lastStep.current = key;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        const prevTarget = params.get("target") === "land" ? "land" : "world";
        if (contextName && target === "world") params.set("name", contextName);
        if (params.get("step") === step && prevTarget === target) return params;
        params.set("step", step);
        if (target === "land") params.set("target", "land");
        else params.delete("target");
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, target, contextName, setSearchParams]);

  const overQuota = exceedsQuota(files, maxFileSizeMb);
  const total = totalBytes(files);

  const landBase = land ? land.baseParcel || land.parcels[0] || "" : "";
  const jumpUrl =
    state.context.result?.jumpUrl ??
    (target === "land"
      ? landBase
        ? landJumpUrl(landBase)
        : undefined
      : state.context.name
        ? worldJumpUrl(state.context.name)
        : undefined);
  const resultProps = {
    chips: {
      network: owner.network,
      address: owner.address,
      username: owner.username,
      verified: owner.verified,
      role: owner.role,
    },
    project,
    files,
    isWorld: target === "world",
    url: jumpUrl,
    sample: false as const,
  };

  return (
    <DeployWorldView
      view={value}
      step={step}
      project={project}
      owner={owner}
      names={names.map((n) => n.name)}
      selectedName={pickedName ?? state.context.name}
      resultProps={resultProps}
      overQuota={overQuota}
      sizeLabel={formatSize(total)}
      maxFileSizeMb={maxFileSizeMb}
      error={state.context.error}
      pendingName={pendingName}
      claimNote={claimNote}
      target={target}
      landOption={
        land ? { baseParcel: landBase, parcelCount: land.parcels.length } : null
      }
      landNotice={landNotice}
      runtimeNote={runtimeNote}
      onChooseWorld={() => send({ type: "CHOOSE_WORLDS" })}
      onChooseLand={() => void chooseLand()}
      onRefresh={pendingName ? refreshNames : undefined}
      onClose={close}
      onBack={() => send({ type: "BACK" })}
      onPickName={(name: string) => void goReview(name)}
      onReview={() =>
        void goReview(
          pickedName ?? state.context.name ?? defaultName ?? "mystore.dcl.eth",
        )
      }
      onClaimName={() => navigate(claimNameUrl(searchParams))}
      onConfirm={() => send({ type: "CONFIRM" })}
      onJumpIn={
        state.context.result?.jumpUrl
          ? () =>
              window.open(
                state.context.result!.jumpUrl,
                "_blank",
                "noopener,noreferrer",
              )
          : undefined
      }
      onRetry={() => send({ type: "RETRY" })}
    />
  );
}
