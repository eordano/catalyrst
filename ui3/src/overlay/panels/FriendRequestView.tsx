import type { ComponentProps } from "react";

import Friends from "../../explorer/pages/Friends";
import ConfirmDialog from "../../explorer/components/ConfirmDialog";
import Spinner from "../../atoms/Spinner";
import "./friendwizard.css";

type FriendSection = NonNullable<
  NonNullable<ComponentProps<typeof Friends>>["initialSection"]
>;
const FRIEND_SECTIONS: readonly FriendSection[] = ["friends", "requests", "blocked"];
function toFriendSection(tab: string | undefined): FriendSection {
  return tab && (FRIEND_SECTIONS as readonly string[]).includes(tab)
    ? (tab as FriendSection)
    : "friends";
}

export type FriendViewAction = "request" | "accept" | "cancel" | "reject" | "block";

export type FriendViewCandidate = {
  address: string;
  name: string;
  mutualCount?: number;
};

function dialogFor(action: FriendViewAction, name: string) {
  switch (action) {
    case "request":
      return {
        title: `Send a friend request to ${name}?`,
        body: "They'll see your request in their Requests tab and can accept or decline.",
        confirmLabel: "SEND REQUEST",
        gradient: "purple" as const,
        danger: false,
      };
    case "accept":
      return {
        title: `Accept ${name}'s friend request?`,
        body: "You'll be able to see each other's status and message in-world.",
        confirmLabel: "ACCEPT",
        gradient: "teal" as const,
        danger: false,
      };
    case "cancel":
    case "reject":
      return {
        title: `Cancel your request to ${name}?`,
        body: "This withdraws the pending friend request.",
        confirmLabel: "CANCEL REQUEST",
        gradient: "purple" as const,
        danger: true,
      };
    case "block":
      return {
        title: `Are you sure you want to block ${name}?`,
        body: "If you block someone in Decentraland, you will no longer see their avatar or messages, and they won't be able to see yours.",
        confirmLabel: "BLOCK",
        gradient: "purple" as const,
        danger: true,
      };
  }
}

function labelFor(action: FriendViewAction): string {
  switch (action) {
    case "request":
      return "a friend request";
    case "accept":
      return "your acceptance";
    case "cancel":
    case "reject":
      return "the cancellation";
    case "block":
      return "the block";
  }
}

function confirmedCopy(action: FriendViewAction): string {
  switch (action) {
    case "request":
      return "Friend request sent to";
    case "accept":
      return "You are now friends with";
    case "cancel":
    case "reject":
      return "Request cancelled for";
    case "block":
      return "You blocked";
  }
}

export type FriendRequestViewProps = {
  value: string;
  action: FriendViewAction;
  candidate: FriendViewCandidate;
  tab?: string;
  error?: string;
  onStart: (action: FriendViewAction) => void;
  onCancel: () => void;
  onConfirm: () => void;
  onRetry: () => void;
};

export default function FriendRequestView({
  value,
  action,
  candidate,
  tab,
  error,
  onStart,
  onCancel,
  onConfirm,
  onRetry,
}: FriendRequestViewProps) {
  return (
    <div className="friend-wizard" data-step={value}>
      <div className="friend-wizard__panel" aria-hidden={value !== "panel"}>
        <Friends initialSection={toFriendSection(tab)} key={tab ?? "friends"} />
        {value === "panel" && (
          <div className="friend-wizard__controls" role="group" aria-label="Friend actions">
            <p className="friend-wizard__hint">
              Demo actions on <strong>{candidate.name}</strong>
              {candidate.mutualCount ? ` · ${candidate.mutualCount} mutual` : ""}
            </p>
            <button
              type="button"
              className="friend-wizard__btn friend-wizard__btn--primary"
              onClick={() => onStart("request")}
            >
              Add friend
            </button>
            <button
              type="button"
              className="friend-wizard__btn"
              onClick={() => onStart("accept")}
            >
              Accept request
            </button>
            <button
              type="button"
              className="friend-wizard__btn"
              onClick={() => onStart("cancel")}
            >
              Cancel / reject
            </button>
            <button
              type="button"
              className="friend-wizard__btn friend-wizard__btn--danger"
              onClick={() => onStart("block")}
            >
              Block
            </button>
          </div>
        )}
      </div>

      {(value === "confirming" || value === "blockPrompt") && (
        (() => {
          const d = dialogFor(value === "blockPrompt" ? "block" : action, candidate.name);
          return (
            <div className="friend-wizard__dialog">
              <ConfirmDialog
                variant="gradient"
                gradient={d.gradient}
                avatar={<span className="friend-wizard__avatar" />}
                title={d.title}
                body={d.body}
                cancelLabel="CANCEL"
                confirmLabel={d.confirmLabel}
                confirmTone={d.danger ? "primary" : undefined}
                onCancel={onCancel}
                onConfirm={onConfirm}
              />
            </div>
          );
        })()
      )}

      {value === "submitting" && (
        <div className="friend-wizard__scrim" role="status" aria-live="polite">
          <div className="friend-wizard__toast">
            <Spinner size={16} color="#fff" aria-hidden="true" />
            Sending {labelFor(action)} to {candidate.name}…
          </div>
        </div>
      )}

      {value === "done" && (
        <div className="friend-wizard__scrim">
          <div className="friend-wizard__toast friend-wizard__toast--ok">
            <span className="friend-wizard__check" aria-hidden="true">✓</span>
            {confirmedCopy(action)} <span className="friend-wizard__user">{candidate.name}</span>
          </div>
          <button
            type="button"
            className="friend-wizard__btn"
            onClick={onCancel}
          >
            Back to Friends
          </button>
        </div>
      )}

      {value === "failed" && (
        <div className="friend-wizard__scrim">
          <div className="friend-wizard__toast friend-wizard__toast--err" role="alert">
            Couldn't complete that action ({error ?? "invalid"}).
          </div>
          <div className="friend-wizard__controls">
            <button
              type="button"
              className="friend-wizard__btn"
              onClick={onCancel}
            >
              Back
            </button>
            <button
              type="button"
              className="friend-wizard__btn friend-wizard__btn--primary"
              onClick={onRetry}
            >
              Retry
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
