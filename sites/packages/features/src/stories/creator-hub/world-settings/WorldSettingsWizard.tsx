import { useCallback, useEffect, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { useRevalidator, useSearchParams } from "react-router";

import ChWorldSettingsTabbedSections, {
  type WorldSceneVM,
} from "@ui/creatorhub/components/ChWorldSettingsTabbedSections";

import { useAuth } from "@data/lib/auth/index";
import { unpublishWorldScene } from "@data/lib/catalyst/creator-hub/unpublish-scene";
import type { TrackContext } from "@core/lib/telemetry/track";
import {
  worldSettingsMachine,
  resolveWorldSettingsSnapshot,
  slugToState,
  stateToSlug,
  tabToUi3,
  type SaveFn,
  type SettingsTab,
  type TrackFn,
} from "./machine";

export type WorldSettingsField = { tab: SettingsTab; field: string; label: string };
export type { WorldSceneVM };

export type WorldSettingsWizardProps = {
  trackCtx: TrackContext;
  worldName: string;
  fields: WorldSettingsField[];
  initialStep?: string;
  scenes?: WorldSceneVM[];
  isOwner?: boolean;
  save?: SaveFn;
  track?: TrackFn;
  onClose?: () => void;
};

export default function WorldSettingsWizard(props: WorldSettingsWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep =
    (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return <WorldSettingsWizardInner stateId={stateId} {...props} />;
}

type InnerProps = WorldSettingsWizardProps & {
  stateId: ReturnType<typeof slugToState>;
};

const TAB_LABEL: Record<SettingsTab, string> = {
  details: "Details",
  layout: "Layout",
  misc: "Misc.",
};

function WorldSettingsWizardInner({
  stateId,
  trackCtx,
  worldName,
  fields,
  scenes,
  isOwner = true,
  save,
  track,
  onClose,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const snapshot = useRef(
    resolveWorldSettingsSnapshot({ step: stateId, trackCtx, worldName, save, track }),
  ).current;

  const [state, send] = useMachine(worldSettingsMachine, {
    input: { trackCtx, worldName, save, track },
    snapshot,
  });

  const { identity } = useAuth();
  const revalidator = useRevalidator();
  const [unpublishingCoord, setUnpublishingCoord] = useState<string | null>(null);
  const [unpublishError, setUnpublishError] = useState<string | null>(null);

  const onUnpublish = useCallback(
    async (coord: string) => {
      if (!identity) {
        setUnpublishError(
          "Connect your wallet to remove a scene from this world.",
        );
        return;
      }
      setUnpublishError(null);
      setUnpublishingCoord(coord);
      try {
        await unpublishWorldScene(worldName, coord, { identity });
        await revalidator.revalidate();
      } catch (err) {
        setUnpublishError(
          err instanceof Error ? err.message : "Failed to remove the scene.",
        );
      } finally {
        setUnpublishingCoord(null);
      }
    },
    [identity, worldName, revalidator],
  );

  const value = state.value as string;
  const step = stateToSlug(value);
  const changes = Object.keys(state.context.changes);
  const hasChanges = changes.length > 0;

  const lastStep = useRef<string | null>(null);
  useEffect(() => {
    if (lastStep.current === step) return;
    lastStep.current = step;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        if (params.get("step") === step) return params;
        params.set("step", step);
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, setSearchParams]);

  const isTabStep = value === "details" || value === "layout" || value === "misc";
  const modalTab: SettingsTab = isTabStep ? (value as SettingsTab) : "misc";
  const tabFields = fields.filter((f) => f.tab === modalTab);

  return (
    <div className="world-settings-wizard" data-step={step}>
      {(isTabStep || value === "review" || !isOwner) && (
        <>
          {isOwner && value !== "layout" && (
            <p
              className="world-settings-wizard__sample"
              role="note"
              style={{
                margin: "0 0 12px",
                padding: "8px 12px",
                borderRadius: "var(--r-control)",
                border: "1px solid color-mix(in srgb, var(--gold) 35%, transparent)",
                background: "color-mix(in srgb, var(--gold) 12%, transparent)",
                color: "var(--gold)",
                fontSize: "12px",
                lineHeight: 1.5,
              }}
            >
              Sample data &#x2014; these values are placeholders; live world settings
              aren&rsquo;t connected yet.
            </p>
          )}
          <ChWorldSettingsTabbedSections
            variant="panel"
            tab={isOwner ? tabToUi3(modalTab) : "layout"}
            isOwner={isOwner}
            hasChanges={isOwner && value === "review"}
            layoutView="scenes"
            worldName={worldName}
            scenes={scenes}
            onUnpublish={onUnpublish}
            unpublishingCoord={unpublishingCoord}
            onClose={onClose}
            onTabChange={
              isOwner
                ? (id: string) =>
                    send({
                      type: "GO_TAB",
                      tab: id === "general" ? "misc" : (id as SettingsTab),
                    })
                : undefined
            }
            onDiscard={() => send({ type: "DISCARD" })}
            onSave={() => send({ type: "SAVE" })}
          />
          {unpublishError && (
            <p className="world-settings-wizard__error" role="alert" style={{ marginTop: 12 }}>
              {unpublishError}
            </p>
          )}
        </>
      )}

      {isOwner && isTabStep && (
        <div
          className="world-settings-wizard__controls"
          role="group"
          aria-label={`${TAB_LABEL[modalTab]} tab actions`}
        >
          {value !== "details" && (
            <button
              type="button"
              className="world-settings-wizard__btn"
              onClick={() => send({ type: "BACK" })}
            >
              Back
            </button>
          )}

          {tabFields.map((f) => (
            <button
              key={`${f.tab}.${f.field}`}
              type="button"
              className={
                "world-settings-wizard__btn" +
                (state.context.changes[`${f.tab}.${f.field}`]
                  ? " world-settings-wizard__btn--dirty"
                  : "")
              }
              onClick={() => send({ type: "CHANGE", tab: f.tab, field: f.field })}
            >
              {state.context.changes[`${f.tab}.${f.field}`] ? "\u{2713} " : "Edit "}
              {f.label}
            </button>
          ))}

          {value === "misc" ? (
            <button
              type="button"
              className="world-settings-wizard__btn world-settings-wizard__btn--primary"
              onClick={() => send({ type: "REVIEW" })}
            >
              Review changes
            </button>
          ) : (
            <button
              type="button"
              className="world-settings-wizard__btn world-settings-wizard__btn--primary"
              onClick={() => send({ type: "NEXT" })}
            >
              {value === "details" ? "Next: Layout" : "Next: Misc."}
            </button>
          )}
        </div>
      )}

      {isOwner && value === "review" && (
        <div className="world-settings-wizard__review" role="status">
          <p className="world-settings-wizard__review-title">
            Review unsaved changes &#x2014; {worldName}
          </p>
          {hasChanges ? (
            <ul className="world-settings-wizard__changelist">
              {changes.map((c) => (
                <li key={c}>
                  <code>{c}</code>
                </li>
              ))}
            </ul>
          ) : (
            <p className="world-settings-wizard__review-sub">No changes to save.</p>
          )}
          <div className="world-settings-wizard__controls">
            <button
              type="button"
              className="world-settings-wizard__btn"
              onClick={() => send({ type: "BACK" })}
            >
              Back
            </button>
            <button
              type="button"
              className="world-settings-wizard__btn world-settings-wizard__btn--text"
              onClick={() => send({ type: "DISCARD" })}
            >
              Discard
            </button>
            <button
              type="button"
              className="world-settings-wizard__btn world-settings-wizard__btn--primary"
              onClick={() => send({ type: "SAVE" })}
            >
              Save changes
            </button>
          </div>
        </div>
      )}

      {isOwner && value === "saving" && (
        <div className="world-settings-wizard__progress" role="status" aria-live="polite">
          <span className="world-settings-wizard__spinner" aria-hidden="true" />
          <p>Saving World settings&#x2026; (write simulated)</p>
        </div>
      )}

      {isOwner && value === "saved" && (
        <div className="world-settings-wizard__saved" role="status">
          <p className="world-settings-wizard__saved-title">
            Settings saved &#x2014; {worldName}
          </p>
          <p className="world-settings-wizard__saved-sub">
            {state.context.result?.savedFields.length ?? 0} field(s) written back
            to the World metadata. (Metadata commit simulated.)
          </p>
          <ul className="world-settings-wizard__changelist">
            {(state.context.result?.savedFields ?? []).map((c) => (
              <li key={c}>
                <code>{c}</code>
              </li>
            ))}
          </ul>
          <div className="world-settings-wizard__controls">
            <a className="world-settings-wizard__btn" href="/create/scenes">
              Back to My Scenes
            </a>
            <a
              className="world-settings-wizard__btn world-settings-wizard__btn--primary"
              href="/creator-hub/world-settings"
            >
              Edit again
            </a>
          </div>
        </div>
      )}

      {isOwner && value === "error" && (
        <div className="world-settings-wizard__progress" role="alert">
          <p className="world-settings-wizard__error">
            Save failed: {state.context.error ?? "unknown error"}
          </p>
          <button
            type="button"
            className="world-settings-wizard__btn world-settings-wizard__btn--primary"
            onClick={() => send({ type: "RETRY" })}
          >
            Retry
          </button>
        </div>
      )}
    </div>
  );
}
