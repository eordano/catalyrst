import { useEffect, useRef } from "react";
import type { ReactNode } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import StCastStreamer from "@ui/web/pages/StCastStreamer";
import StCastNotFound from "@ui/web/pages/StCastNotFound";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  castMachine,
  resolveCastSnapshot,
  slugToState,
  stateToSlug,
  DEFAULT_DEVICES,
  type DeviceSelection,
  type EndCastFn,
  type RequestPermissionsFn,
  type ResolveTokenFn,
  type ShareScreenFn,
  type TrackFn,
} from "./machine";

export type CastConsoleProps = {
  trackCtx: TrackContext;
  token: string;
  identity?: string;
  initialStep?: string;
  resolveToken?: ResolveTokenFn;
  requestPermissions?: RequestPermissionsFn;
  endCast?: EndCastFn;
  shareScreen?: ShareScreenFn;
  track?: TrackFn;
};

export default function CastConsole({
  trackCtx,
  token,
  identity,
  initialStep,
  resolveToken,
  requestPermissions,
  endCast,
  shareScreen,
  track,
}: CastConsoleProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <CastConsoleInner
      key={stateId}
      stateId={stateId}
      trackCtx={trackCtx}
      token={token}
      identity={identity}
      resolveToken={resolveToken}
      requestPermissions={requestPermissions}
      endCast={endCast}
      shareScreen={shareScreen}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  trackCtx: TrackContext;
  token: string;
  identity?: string;
  resolveToken?: ResolveTokenFn;
  requestPermissions?: RequestPermissionsFn;
  endCast?: EndCastFn;
  shareScreen?: ShareScreenFn;
  track?: TrackFn;
};

function CastConsoleInner({
  stateId,
  trackCtx,
  token,
  identity,
  resolveToken,
  requestPermissions,
  endCast,
  shareScreen,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const snapshot = useRef(
    resolveCastSnapshot({
      step: stateId,
      trackCtx,
      token,
      identity,
      resolveToken,
      requestPermissions,
      endCast,
      shareScreen,
      track,
    }),
  ).current;

  const [state, send] = useMachine(castMachine, {
    input: {
      trackCtx,
      token,
      identity,
      resolveToken,
      requestPermissions,
      endCast,
      shareScreen,
      track,
    },
    snapshot,
  });

  const value = state.matches("live") ? "live" : (state.value as string);
  const step = stateToSlug(value);

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

  const placeName = state.context.info?.placeName ?? "Genesis Plaza";
  const displayName = state.context.identity || identity || "";

  return (
    <div className="cast-console" data-step={step}>
      {value === "tokenCheck" && (
        <StCastStreamer state="joining" streamName={placeName} toasts={[]} />
      )}

      {value === "deviceSelect" && (
        <>
          <StCastStreamer
            state="onboarding"
            streamName={placeName}
            displayName={displayName}
            toasts={[]}
          />
          <ConsoleControls label="Devices">
            <button
              type="button"
              className="cast-console__btn cast-console__btn--primary"
              onClick={() => send({ type: "SELECT_DEVICES", devices: DEFAULT_DEVICES })}
            >
              Continue with these devices
            </button>
          </ConsoleControls>
        </>
      )}

      {value === "permissions" && (
        <>
          <StCastStreamer
            state="joining"
            streamName={placeName}
            toasts={
              state.context.permissionsDenied
                ? [
                    {
                      id: "perm",
                      title: "Camera & microphone blocked",
                      message:
                        "Allow access to your camera and microphone, then retry to continue.",
                      action: { label: "Retry", onClick: () => {} },
                    },
                  ]
                : []
            }
          />
          <ConsoleControls label="Permissions">
            {state.context.permissionsDenied ? (
              <>
                <button
                  type="button"
                  className="cast-console__btn"
                  onClick={() => send({ type: "BACK" })}
                >
                  Change devices
                </button>
                <button
                  type="button"
                  className="cast-console__btn cast-console__btn--primary"
                  onClick={() => send({ type: "RETRY_PERMISSIONS" })}
                >
                  Allow access
                </button>
              </>
            ) : (
              <span className="cast-console__hint">Requesting camera & microphone&#x2026;</span>
            )}
          </ConsoleControls>
        </>
      )}

      {value === "preview" && (
        <>
          <StCastStreamer
            state="onboarding"
            streamName={placeName}
            displayName={displayName}
            toasts={[]}
          />
          <ConsoleControls label="Preview">
            <button
              type="button"
              className="cast-console__btn"
              onClick={() => send({ type: "BACK" })}
            >
              Back
            </button>
            <button
              type="button"
              className="cast-console__btn cast-console__btn--primary"
              onClick={() => send({ type: "JOIN" })}
            >
              Join now
            </button>
          </ConsoleControls>
        </>
      )}

      {value === "live" &&
        (() => {
          const sharing = state.matches({ live: "sharing" });
          const publishing = state.matches({ live: "publishingShare" });
          return (
            <>
              <StCastStreamer
                state="live"
                streamName={placeName}
                displayName={displayName || "Streamer"}
                participants={1}
                toasts={
                  state.context.screenShareFailed
                    ? [
                        {
                          id: "ss",
                          title: "Screen sharing failed",
                          message:
                            "Your screen share stopped. Click retry to share again.",
                          action: {
                            label: "Retry",
                            onClick: () => send({ type: "TOGGLE_SCREENSHARE" }),
                          },
                        },
                      ]
                    : []
                }
              />
              <ConsoleControls label="Live">
                <button
                  type="button"
                  className={
                    "cast-console__btn" + (sharing ? " cast-console__btn--active" : "")
                  }
                  disabled={publishing}
                  onClick={() => send({ type: "TOGGLE_SCREENSHARE" })}
                >
                  {sharing
                    ? "Stop sharing"
                    : publishing
                      ? "Starting share\u{2026}"
                      : "Share screen"}
                </button>
                <button
                  type="button"
                  className="cast-console__btn cast-console__btn--primary"
                  onClick={() => send({ type: "LEAVE" })}
                >
                  Leave stream
                </button>
              </ConsoleControls>
            </>
          );
        })()}

      {value === "ending" && (
        <StCastStreamer
          state="joining"
          streamName={placeName}
          toasts={[
            {
              id: "ending",
              title: "Ending your cast\u{2026}",
              message: "Closing the room and releasing your devices.",
              action: { label: "OK", onClick: () => {} },
            },
          ]}
        />
      )}

      {value === "ended" && (
        <StCastStreamer
          state="error"
          streamName={placeName}
          errorTitle="Your cast has ended"
          errorMessage="Thanks for casting. Start a new cast from the Admin Smart Item in your scene."
          toasts={[]}
        />
      )}

      {value === "invalid" && (
        <>
          <StCastNotFound
            description={
              (state.context.invalidReason ?? "Invalid streaming key") +
              ". Stream links are generated on-demand by the Admin Smart Item in Decentraland scenes."
            }
          />
          <ConsoleControls label="Invalid link">
            <button
              type="button"
              className="cast-console__btn cast-console__btn--primary"
              onClick={() => send({ type: "RETRY" })}
            >
              Try again
            </button>
          </ConsoleControls>
        </>
      )}
    </div>
  );
}

function ConsoleControls({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <div className="cast-console__controls" role="group" aria-label={label}>
      {children}
    </div>
  );
}

export type { DeviceSelection };
