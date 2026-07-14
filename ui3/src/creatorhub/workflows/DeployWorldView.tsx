import type { ComponentProps } from "react";

import ChPublishWizardPublishToWorld from "./ChPublishWizardPublishToWorld";
import ChPublishWizardDeployProgressResult from "./ChPublishWizardDeployProgressResult";
import Button from "../../atoms/Button";
import "./deployworldview.css";

type LandOption = { baseParcel: string; parcelCount: number };

type DeployWorldViewProps = {
  view?: string;
  step?: string;
  project?: ComponentProps<typeof ChPublishWizardPublishToWorld>["project"];
  owner?: ComponentProps<typeof ChPublishWizardPublishToWorld>["owner"];
  names?: string[];
  selectedName?: string;
  resultProps?: ComponentProps<typeof ChPublishWizardDeployProgressResult>;
  overQuota?: boolean;
  sizeLabel?: string;
  maxFileSizeMb?: number;
  error?: string;
  pendingName?: string;
  claimNote?: string;
  target?: "world" | "land";
  landOption?: LandOption | null;
  landNotice?: string;
  runtimeNote?: string;
  onChooseWorld?: () => void;
  onChooseLand?: () => void;
  onRefresh?: () => void;
  onClose?: () => void;
  onBack?: () => void;
  onPickName?: (name: string) => void;
  onReview?: () => void;
  onClaimName?: () => void;
  onConfirm?: () => void;
  onJumpIn?: () => void;
  onRetry?: () => void;
};

const LAND_NETWORK_NOTE =
  "Publishing updates LAND on this network (catalyst.example.com's catalyst) only. " +
  "Genesis City on decentraland.org is not affected.";

export default function DeployWorldView({
  view = "destination",
  step = "destination",
  project = {},
  owner = {},
  names = [],
  selectedName = undefined,
  resultProps = {},
  overQuota = false,
  sizeLabel = "",
  maxFileSizeMb = 50,
  error = undefined,
  pendingName = undefined,
  claimNote = undefined,
  target = "world",
  landOption = null,
  landNotice = undefined,
  runtimeNote = undefined,
  onChooseWorld = undefined,
  onChooseLand = undefined,
  onRefresh = undefined,
  onClose = undefined,
  onBack = undefined,
  onPickName = undefined,
  onReview = undefined,
  onClaimName = undefined,
  onConfirm = undefined,
  onJumpIn = undefined,
  onRetry = undefined,
}: DeployWorldViewProps) {
  return (
    <div className="deploy-world-wizard" data-step={step}>
      {view === "destination" && (
        <div
          className="deploy-world-wizard__destination"
          role="group"
          aria-label="Choose a destination"
        >
          <h2 className="deploy-world-wizard__destination-title">
            Where do you want to publish?
          </h2>
          <div className="deploy-world-wizard__destination-cards">
            <button
              type="button"
              className="deploy-world-wizard__destcard"
              onClick={onChooseWorld}
            >
              <span className="deploy-world-wizard__destcard-kind">World</span>
              <span className="deploy-world-wizard__destcard-title">
                Publish to a World
              </span>
              <span className="deploy-world-wizard__destcard-desc">
                Your scene goes live under a NAME you own, on this
                network&apos;s worlds server.
              </span>
            </button>
            {landOption && (
              <button
                type="button"
                className="deploy-world-wizard__destcard deploy-world-wizard__destcard--land"
                onClick={onChooseLand}
              >
                <span className="deploy-world-wizard__destcard-kind">LAND</span>
                <span className="deploy-world-wizard__destcard-title">
                  Publish to LAND {landOption.baseParcel}
                </span>
                <span className="deploy-world-wizard__destcard-desc">
                  {landOption.parcelCount > 1
                    ? `Updates all ${landOption.parcelCount} parcels of this scene `
                    : "Updates this parcel "}
                  on catalyst.example.com&apos;s catalyst. Genesis City on decentraland.org
                  is not affected.
                </span>
              </button>
            )}
          </div>
          {landNotice && (
            <p className="deploy-world-wizard__landnotice" role="note">
              {landNotice}
            </p>
          )}
        </div>
      )}

      {landNotice && (view === "selectWorld" || view === "namesEmpty") && (
        <p className="deploy-world-wizard__landnotice" role="note">
          {landNotice}
        </p>
      )}

      {view === "selectWorld" && (
        <ChPublishWizardPublishToWorld
          state="selection"
          inline
          project={project}
          owner={owner}
          names={names}
          selectedName={selectedName}
          world={null}
          pendingName={pendingName}
          claimNote={claimNote}
          onRefresh={onRefresh}
          onPickName={onPickName}
          onReview={onReview}
          onClose={onClose}
          onClaimName={onClaimName}
        />
      )}

      {view === "namesEmpty" && (
        <ChPublishWizardPublishToWorld
          state="empty"
          inline
          pendingName={pendingName}
          claimNote={claimNote}
          onRefresh={onRefresh}
          onClose={onClose}
          onClaimName={onClaimName}
        />
      )}

      {view === "review" && (
        <>
          {overQuota && (
            <div className="deploy-world-wizard__quota-banner" role="alert">
              <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
                <path
                  d="M12 3.2 22 20.5H2L12 3.2Z"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.8"
                  strokeLinejoin="round"
                />
                <path d="M12 9.5v4.6M12 17v.05" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
              </svg>
              <span>
                This scene is <strong>{sizeLabel}</strong> &#x2014; over the {maxFileSizeMb}MB
                {target === "land" ? " publish" : " world"} limit. Optimize or remove
                large files before you can publish.
              </span>
            </div>
          )}
          <ChPublishWizardDeployProgressResult
            state={overQuota ? "exceeded" : "idle"}
            {...resultProps}
          />
          {runtimeNote && (
            <p className="deploy-world-wizard__netnote deploy-world-wizard__netnote--block" role="note">
              {runtimeNote}
            </p>
          )}
          <div className="deploy-world-wizard__controls" role="group" aria-label="Review and confirm">
            {target === "land" && (
              <span className="deploy-world-wizard__netnote" role="note">
                {LAND_NETWORK_NOTE}
              </span>
            )}
            <Button variant="secondary" onClick={onBack}>
              Back
            </Button>
            <span className="deploy-world-wizard__quota">
              {sizeLabel} / {maxFileSizeMb}MB
              {overQuota ? " \u{2014} over quota" : ""}
            </span>
            <Button variant="primary" disabled={overQuota} onClick={onConfirm}>
              Publish to{" "}
              {target === "land"
                ? `LAND ${landOption?.baseParcel ?? selectedName ?? ""}`.trim()
                : (selectedName ?? "World")}
            </Button>
          </div>
        </>
      )}

      {view === "deploying" && (
        <ChPublishWizardDeployProgressResult state="deploying" {...resultProps} />
      )}
      {view === "finishing" && (
        <ChPublishWizardDeployProgressResult state="finishing" {...resultProps} />
      )}
      {view === "complete" && (
        <>
          <ChPublishWizardDeployProgressResult
            state="complete"
            {...resultProps}
            onJumpIn={onJumpIn}
          />
          {target === "land" && (
            <p className="deploy-world-wizard__netnote deploy-world-wizard__netnote--block" role="note">
              {LAND_NETWORK_NOTE}
            </p>
          )}
        </>
      )}
      {view === "unavailable" && (
        <div className="deploy-world-wizard__unavailable" role="alert">
          <h2 className="deploy-world-wizard__unavailable-title">
            Publishing from the web needs a bit more
          </h2>
          <p className="deploy-world-wizard__unavailable-body">
            To publish a scene to your World from the browser you need a
            connected wallet and your scene&apos;s project folder open on disk.
            Sign in and select the folder that contains your{" "}
            <code>scene.json</code>, then try again.
          </p>
          <div
            className="deploy-world-wizard__controls"
            role="group"
            aria-label="Publishing unavailable"
          >
            <Button variant="primary" onClick={onClose}>
              Done
            </Button>
          </div>
        </div>
      )}
      {view === "error" && (
        <ChPublishWizardDeployProgressResult
          state="error"
          {...resultProps}
          error={error ? { message: error } : undefined}
          onRetry={onRetry}
        />
      )}
    </div>
  );
}
