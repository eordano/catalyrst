import { useEffect, useRef, useState, type ReactNode } from "react";
import { useMachine } from "@xstate/react";
import { useNavigate, useSearchParams } from "react-router";

import { useProfileName } from "@data/lib/auth/use-profile-name";

import ChWorldPermissionsTabbedSections from "@ui/creatorhub/components/ChWorldPermissionsTabbedSections";
import ChModalWorldPermissions from "@ui/creatorhub/components/ChModalWorldPermissions";
import ChWorldPermissionsSetChangePasswordDialo from "@ui/creatorhub/components/ChWorldPermissionsSetChangePasswordDialo";
import ChWorldPermissionsAddCollaboratorDialog from "@ui/creatorhub/components/ChWorldPermissionsAddCollaboratorDialog";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  isValidAddress,
  type WorldPermissions,
} from "@data/lib/catalyst/creator-hub/world-permissions";
import {
  permissionsMachine,
  resolvePermissionsSnapshot,
  slugToState,
  stateToSlug,
  type AccessType,
  type CommitFn,
  type InviteChannel,
  type TrackFn,
} from "./machine";

export type WorldPermissionsWizardProps = {
  trackCtx: TrackContext;
  data: WorldPermissions;
  initialStep?: string;
  commit?: CommitFn;
  track?: TrackFn;
};

export default function WorldPermissionsWizard({
  trackCtx,
  data,
  initialStep,
  commit,
  track,
}: WorldPermissionsWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  const ownerName = useProfileName(data.owner, Boolean(data.owner));

  return (
    <WorldPermissionsWizardInner
      stateId={stateId}
      trackCtx={trackCtx}
      data={data}
      ownerName={ownerName}
      commit={commit}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  trackCtx: TrackContext;
  data: WorldPermissions;
  ownerName: string;
  commit?: CommitFn;
  track?: TrackFn;
};

function WorldPermissionsWizardInner({
  stateId,
  trackCtx,
  data,
  ownerName,
  commit,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const navigate = useNavigate();

  const accessType = data.permissions.access.type as AccessType;
  const seededCollaborators = data.collaborators.map((c) => c.address);

  const snapshot = useRef(
    resolvePermissionsSnapshot({
      step: stateId,
      trackCtx,
      accessType,
      collaborators: seededCollaborators,
      commit,
      track,
    }),
  ).current;

  const [state, send] = useMachine(permissionsMachine, {
    input: {
      trackCtx,
      accessType,
      collaborators: seededCollaborators,
      commit,
      track,
    },
    snapshot,
  });

  const value = state.value as string;
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

  const worldName = data.world.name;
  const ownerAddress = data.owner ?? undefined;

  const accessWallets = data.permissions.access.wallets.map((address) => {
    const c = data.collaborators.find((x) => x.address === address);
    return { address, name: c?.name ?? undefined, role: c?.role };
  });

  const collaborators = data.collaborators.map((c) => ({
    address: c.address,
    name: c.name ?? undefined,
    deployment: c.deployment,
    parcelsCount: c.parcels,
  }));

  return (
    <div className="world-perms-wizard" data-step={step}>
      {value === "access" && (
        <>
          <ChWorldPermissionsTabbedSections
            variant="panel"
            worldName={worldName}
            initialTab="access"
            accessType={accessType}
            ownerAddress={ownerAddress}
            ownerName={(ownerName || undefined) as undefined}
            accessWallets={accessWallets as never[]}
            collaborators={collaborators}
            onClose={() => navigate(-1)}
            onNewInvite={() => send({ type: "START_INVITE" })}
          />
          <WizardControls label="Access actions">
            <WizardBtn onClick={() => send({ type: "START_INVITE" })} primary>
              New Invite
            </WizardBtn>
            <WizardBtn onClick={() => send({ type: "OPEN_PASSWORD" })}>
              Set / Change Password
            </WizardBtn>
          </WizardControls>
        </>
      )}

      {value === "invite" && (
        <>
          <InviteSurface />
          <WizardControls label="New invite">
            <WizardBtn onClick={() => send({ type: "BACK" })}>Back</WizardBtn>
            <WizardBtn onClick={() => send({ type: "SUBMIT_INVITE", channel: inviteChannelRef.current })}>
              Submit invite
            </WizardBtn>
          </WizardControls>
        </>
      )}

      {value === "password" && (
        <PasswordStep
          isChanging={accessType === "shared-secret"}
          onBack={() => send({ type: "BACK" })}
          onSave={() => send({ type: "SET_PASSWORD" })}
        />
      )}

      {value === "collaborators" && (
        <>
          <ChWorldPermissionsTabbedSections
            variant="panel"
            worldName={worldName}
            initialTab="collaborators"
            accessType={accessType}
            ownerAddress={ownerAddress}
            ownerName={(ownerName || undefined) as undefined}
            accessWallets={accessWallets as never[]}
            collaborators={collaborators}
            onClose={() => navigate(-1)}
            onAddCollaborator={() => send({ type: "ADD" })}
          />
          <WizardControls label="Collaborator actions">
            <WizardBtn onClick={() => send({ type: "BACK" })}>Back</WizardBtn>
            <WizardBtn onClick={() => send({ type: "ADD" })}>
              Add Collaborator
            </WizardBtn>
            <WizardBtn onClick={() => send({ type: "CONFIRM" })} primary>
              Save permissions
            </WizardBtn>
          </WizardControls>
        </>
      )}

      {value === "addingCollaborator" && (
        <AddCollaboratorStep
          error={state.context.addressError}
          candidate={state.context.candidate}
          onCancel={() => send({ type: "CANCEL" })}
          onValidate={(address) => send({ type: "VALIDATE", address })}
        />
      )}

      {(value === "confirming" ||
        value === "finishing" ||
        value === "complete" ||
        value === "error") && (
        <CommitOverlay
          state={value}
          error={state.context.error}
          addresses={state.context.result?.addresses}
          stub={state.context.result?.stub}
          onRetry={() => send({ type: "RETRY" })}
        />
      )}
    </div>
  );
}

const inviteChannelRef = { current: "wallet" as InviteChannel };

function InviteSurface() {
  const [channel, setChannel] = useState<InviteChannel>("wallet");
  inviteChannelRef.current = channel;
  const tabs: InviteChannel[] = ["wallet", "community", "csv"];
  return (
    <div className="world-perms-wizard__invite">
      <div
        className="world-perms-wizard__channels"
        role="tablist"
        aria-label="Invite channel"
      >
        {tabs.map((t) => (
          <button
            key={t}
            type="button"
            role="tab"
            aria-selected={t === channel}
            className={
              "world-perms-wizard__channel" + (t === channel ? " is-active" : "")
            }
            onClick={() => setChannel(t)}
          >
            {t === "wallet" ? "Wallet Address" : t === "community" ? "Community" : "Import CSV"}
          </button>
        ))}
      </div>
      <ChModalWorldPermissions variant="panel" tab="access" view="invite_form" inviteTab={channel} />
    </div>
  );
}

const PASSWORD_MIN_LENGTH = 8;
const PASSWORD_MIN_NUMBERS = 2;

export function isValidWorldPassword(password: string, confirm: string): boolean {
  return (
    password.length >= PASSWORD_MIN_LENGTH &&
    (password.match(/\d/g) ?? []).length >= PASSWORD_MIN_NUMBERS &&
    password === confirm
  );
}

function PasswordStep({
  isChanging,
  onBack,
  onSave,
}: {
  isChanging: boolean;
  onBack: () => void;
  onSave: () => void;
}) {
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const valid = isValidWorldPassword(password, confirm);
  const mismatch = confirm.length > 0 && password !== confirm;

  return (
    <>
      <ChWorldPermissionsSetChangePasswordDialo variant="panel" isChanging={isChanging} />
      <div className="world-perms-wizard__addfields">
        <label className="world-perms-wizard__addlabel" htmlFor="wp-password">
          Access password (min {PASSWORD_MIN_LENGTH} characters, at least{" "}
          {PASSWORD_MIN_NUMBERS} numbers)
        </label>
        <input
          id="wp-password"
          className="world-perms-wizard__addinput"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          aria-invalid={password.length > 0 && !valid}
        />
        <label className="world-perms-wizard__addlabel" htmlFor="wp-password-confirm">
          Repeat password
        </label>
        <input
          id="wp-password-confirm"
          className="world-perms-wizard__addinput"
          type="password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          aria-invalid={mismatch}
        />
        {mismatch && (
          <p className="world-perms-wizard__adderror" role="alert">
            Passwords do not match
          </p>
        )}
      </div>
      <WizardControls label="Password">
        <WizardBtn onClick={onBack}>Back</WizardBtn>
        <WizardBtn onClick={onSave} primary disabled={!valid}>
          Save password
        </WizardBtn>
      </WizardControls>
    </>
  );
}

function AddCollaboratorStep({
  error,
  candidate,
  onCancel,
  onValidate,
}: {
  error?: string;
  candidate?: string;
  onCancel: () => void;
  onValidate: (address: string) => void;
}) {
  const [address, setAddress] = useState(candidate ?? "");
  const valid = isValidAddress(address);
  return (
    <div className="world-perms-wizard__addcollab">
      <ChWorldPermissionsAddCollaboratorDialog variant="panel" value={address} error={(error ?? null) as null} />
      <div className="world-perms-wizard__addfields">
        <label className="world-perms-wizard__addlabel" htmlFor="wp-collab-addr">
          Wallet address
        </label>
        <input
          id="wp-collab-addr"
          className="world-perms-wizard__addinput"
          type="text"
          placeholder="0x..."
          value={address}
          onChange={(e) => setAddress(e.target.value)}
          aria-invalid={!!error}
        />
        {error && (
          <p className="world-perms-wizard__adderror" role="alert">
            {error}
          </p>
        )}
      </div>
      <WizardControls label="Add collaborator">
        <WizardBtn onClick={onCancel}>Cancel</WizardBtn>
        <WizardBtn onClick={() => onValidate(address)} primary disabled={!valid}>
          Confirm
        </WizardBtn>
      </WizardControls>
    </div>
  );
}

function CommitOverlay({
  state,
  error,
  addresses,
  stub,
  onRetry,
}: {
  state: string;
  error?: string;
  addresses?: number;
  stub?: boolean;
  onRetry: () => void;
}) {
  const copy: Record<string, { title: string; body: string }> = stub
    ? {
        confirming: {
          title: "Saving permissions\u{2026}",
          body: "Writing the access control list (simulated PUT /world/.../permissions).",
        },
        finishing: {
          title: "Finalizing\u{2026}",
          body: "Propagating the updated ACL across the worlds content server (simulated).",
        },
        complete: {
          title: "Permissions saved",
          body: `Access control updated for ${addresses ?? 0} address(es). This write is simulated \u{2014} no worlds-content-server change was made.`,
        },
        error: {
          title: "Could not save permissions",
          body: error ?? "The simulated ACL write failed.",
        },
      }
    : {
        confirming: {
          title: "Saving permissions\u{2026}",
          body: "Signing and writing the access control list (POST /world/.../permissions/access).",
        },
        finishing: {
          title: "Finalizing\u{2026}",
          body: "Applying the updated ACL on the worlds content server.",
        },
        complete: {
          title: "Permissions saved",
          body: `Access control updated for ${addresses ?? 0} address(es) on the worlds content server.`,
        },
        error: {
          title: "Could not save permissions",
          body: error ?? "The ACL write failed.",
        },
      };
  const c = copy[state] ?? copy.confirming;
  return (
    <div className="world-perms-wizard__overlay" data-state={state}>
      <h2 className="world-perms-wizard__overlaytitle">{c.title}</h2>
      <p className="world-perms-wizard__overlaybody">{c.body}</p>
      {stub ? (
        <p className="world-perms-wizard__stubnote">
          Simulated ACL write &#x2014; the real route is{" "}
          <code>POST /world/&lt;name&gt;/permissions/access</code>.
        </p>
      ) : null}
      {state === "error" && (
        <WizardControls label="Retry">
          <WizardBtn onClick={onRetry} primary>
            Retry
          </WizardBtn>
        </WizardControls>
      )}
    </div>
  );
}

function WizardControls({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <div className="world-perms-wizard__controls" role="group" aria-label={label}>
      {children}
    </div>
  );
}

function WizardBtn({
  children,
  onClick,
  primary,
  disabled,
}: {
  children: ReactNode;
  onClick: () => void;
  primary?: boolean;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      className={
        "world-perms-wizard__btn" + (primary ? " world-perms-wizard__btn--primary" : "")
      }
      onClick={onClick}
      disabled={disabled}
    >
      {children}
    </button>
  );
}
