import type React from "react";

import CommunityCreate from "../../explorer/components/CommunityCreate";
import Dropdown from "../../components/Dropdown";
import Checkbox from "../../atoms/Checkbox";
import "../../explorer/components/communitycreate.css";
import "./communitycreateview.css";

export type CommunityPrivacy = "public" | "private";
export type CommunityVisibility = "all" | "unlisted";

export type CommunityDraft = {
  name: string;
  description: string;
  privacy: CommunityPrivacy;
  visibility: CommunityVisibility;
  hasThumbnail: boolean;
  policyAck: boolean;
};

export type CommunityDraftIssues = Partial<Record<keyof CommunityDraft, string>>;

export type CommunityMembershipOption = {
  value: CommunityPrivacy;
  label: string;
  note: string;
};

export type CommunityCreateViewProps = {
  value: string;
  step: string;
  hasName: boolean;
  draft: CommunityDraft;
  issues: CommunityDraftIssues;
  error?: string;
  resultId?: string;
  membershipOptions: CommunityMembershipOption[];
  crumbLabels: Record<string, string>;
  nameMax: number;
  onOpen: () => void;
  onGetName: () => void;
  onBack: () => void;
  onNext: () => void;
  onEdit: (patch: Partial<CommunityDraft>) => void;
  onSubmit: () => void;
};

export default function CommunityCreateView({
  value,
  step,
  hasName,
  draft,
  issues,
  error,
  resultId,
  membershipOptions,
  crumbLabels,
  nameMax,
  onOpen,
  onGetName,
  onBack,
  onNext,
  onEdit,
  onSubmit,
}: CommunityCreateViewProps) {
  return (
    <div className="cc-create" data-step={step} data-has-name={hasName}>
      <CommunityCreate />

      <div className="cc-create__controls" role="group" aria-label="Create a community">
        <Breadcrumbs active={value} hasName={hasName} labels={crumbLabels} />

        {value === "create" && <CreateControls onOpen={onOpen} />}

        {value === "gate" && (
          <GateControls onGetName={onGetName} onBack={onBack} />
        )}

        {value === "profile" && (
          <ProfileControls
            draft={draft}
            onEdit={onEdit}
            onBack={onBack}
            onNext={onNext}
          />
        )}

        {value === "details" && (
          <DetailsControls
            draft={draft}
            issues={issues}
            nameMax={nameMax}
            membershipOptions={membershipOptions}
            onEdit={onEdit}
            onBack={onBack}
            onNext={onNext}
          />
        )}

        {value === "review" && (
          <ReviewControls
            draft={draft}
            error={error}
            onEdit={onEdit}
            onBack={onBack}
            onSubmit={onSubmit}
          />
        )}

        {value === "submit" && (
          <p className="cc-create__hint" role="status">
            Creating your community… (the signed create is <strong>simulated</strong>)
          </p>
        )}

        {value === "done" && <DoneControls id={resultId} />}
      </div>
    </div>
  );
}

function Breadcrumbs({
  active,
  hasName,
  labels,
}: {
  active: string;
  hasName: boolean;
  labels: Record<string, string>;
}) {
  const order = ["create", "gate", "profile", "details", "review", "done"];
  return (
    <div className="cc-create__crumbs" aria-hidden="true">
      {order.map((s) => {
        const skipped = s === "gate" && hasName;
        return (
          <span
            key={s}
            className={
              "cc-create__crumb" +
              (s === active ? " is-active" : "") +
              (skipped ? " is-skipped" : "")
            }
          >
            {labels[s] ?? s}
          </span>
        );
      })}
    </div>
  );
}

function CreateControls({ onOpen }: { onOpen: () => void }) {
  return (
    <>
      <p className="cc-create__hint">
        Open the <strong>Create a Community</strong> panel over the communities
        directory.
      </p>
      <div className="cc-create__row">
        <button
          type="button"
          className="cc-create__btn cc-create__btn--primary"
          onClick={onOpen}
        >
          Create a community
        </button>
      </div>
    </>
  );
}

function GateControls({
  onGetName,
  onBack,
}: {
  onGetName: () => void;
  onBack: () => void;
}) {
  return (
    <>
      <p className="cc-create__hint">
        <strong>Get a NAME to Unlock Community Creation.</strong> Creation is gated
        on holding a claimed Decentraland NAME (<code>require_owned_name</code>).
        The NAME purchase + check are <strong>simulated</strong> here.
      </p>
      <div className="cc-create__row">
        <button type="button" className="cc-create__btn" onClick={onBack}>
          Maybe later
        </button>
        <button
          type="button"
          className="cc-create__btn cc-create__btn--primary"
          onClick={onGetName}
        >
          Get a NAME &amp; continue
        </button>
      </div>
    </>
  );
}

type StepControlsProps = {
  draft: CommunityDraft;
  issues?: CommunityDraftIssues;
  onEdit: (patch: Partial<CommunityDraft>) => void;
  onBack: () => void;
  onNext: () => void;
};

function ProfileControls({ draft, onEdit, onBack, onNext }: StepControlsProps) {
  return (
    <>
      <div className="cc-create__fields">
        <span className="cc-create__label">Profile picture</span>
        <span className="cc-create__hint">PNG or JPG | 512x512 px | 500KB max (optional)</span>
        <div className="cc-create__toggle">
          <Checkbox
            checked={draft.hasThumbnail}
            onChange={(checked) => onEdit({ hasThumbnail: checked })}
          >
            I&apos;ve chosen a 512x512 thumbnail (upload simulated)
          </Checkbox>
        </div>
      </div>
      <NavRow onBack={onBack} onNext={onNext} canNext />
    </>
  );
}

type DetailsControlsProps = StepControlsProps & {
  nameMax: number;
  membershipOptions: CommunityMembershipOption[];
};

function DetailsControls({
  draft,
  issues = {},
  nameMax,
  membershipOptions,
  onEdit,
  onBack,
  onNext,
}: DetailsControlsProps) {
  const canNext = !issues.name && draft.name.trim() !== "";

  const labels = membershipOptions.map((o) => o.label);
  const labelToValue = new Map(membershipOptions.map((o) => [o.label, o.value]));
  const valueToLabel = new Map(membershipOptions.map((o) => [o.value, o.label]));

  return (
    <>
      <div className="cc-create__fields">
        <label className="cc-create__field">
          <span className="cc-create__label">
            Community name <span aria-hidden="true">*</span>
          </span>
          <input
            className="cc-create__input"
            maxLength={nameMax}
            placeholder="Write here"
            value={draft.name}
            onChange={(e) => onEdit({ name: e.target.value })}
          />
          {issues.name && <span className="cc-create__error">{issues.name}</span>}
        </label>

        <div className="cc-create__field">
          <span className="cc-create__label">Membership</span>
          <Dropdown
            {...({
              options: labels,
              value: valueToLabel.get(draft.privacy) ?? labels[0],
              onChange: (label: string) =>
                onEdit({ privacy: (labelToValue.get(label) ?? "public") as CommunityPrivacy }),
            } as unknown as React.ComponentProps<typeof Dropdown>)}
          />
        </div>

        <div className="cc-create__field">
          <span className="cc-create__label">Discoverability</span>
          <label className="cc-create__toggle">
            <input
              type="checkbox"
              checked={draft.visibility === "all"}
              onChange={(e) =>
                onEdit({
                  visibility: (e.target.checked ? "all" : "unlisted") as CommunityVisibility,
                })
              }
            />
            Listed in the public directory (off = unlisted)
          </label>
        </div>
      </div>
      <NavRow onBack={onBack} onNext={onNext} canNext={canNext} />
    </>
  );
}

function ReviewControls({
  draft,
  error,
  onEdit,
  onBack,
  onSubmit,
}: {
  draft: CommunityDraft;
  error?: string;
  onEdit: (patch: Partial<CommunityDraft>) => void;
  onBack: () => void;
  onSubmit: () => void;
}) {
  const canSubmit = draft.policyAck && draft.name.trim() !== "";
  return (
    <>
      <div className="cc-create__fields">
        <span className="cc-create__label">Review</span>
        <span className="cc-create__hint">
          <strong>{draft.name || "(unnamed)"}</strong> · {draft.privacy} ·{" "}
          {draft.visibility === "all" ? "listed" : "unlisted"}
          {draft.hasThumbnail ? " · thumbnail" : ""}
        </span>
        <div className="cc-create__toggle">
          <Checkbox
            checked={draft.policyAck}
            onChange={(checked) => onEdit({ policyAck: checked })}
          >
            I acknowledge this community must follow Decentraland&apos;s Content Policy.
          </Checkbox>
        </div>
        {error && (
          <span className="cc-create__error">Create failed: {error}. Try again.</span>
        )}
      </div>
      <p className="cc-create__sim">
        The signed <code>POST /v1/communities</code> create is{" "}
        <strong>simulated</strong> (it needs a DCL auth-chain signature + a held
        NAME, and is not exposed on the public edge).
      </p>
      <div className="cc-create__row">
        <button type="button" className="cc-create__btn" onClick={onBack}>
          Back
        </button>
        <button
          type="button"
          className="cc-create__btn cc-create__btn--primary"
          aria-disabled={!canSubmit}
          disabled={!canSubmit}
          onClick={onSubmit}
        >
          Create community
        </button>
      </div>
    </>
  );
}

function DoneControls({ id }: { id?: string }) {
  return (
    <>
      <p className="cc-create__hint" role="status">
        Community created (simulated). Id: <code>{id ? `${id.slice(0, 12)}…` : "—"}</code>
      </p>
      <div className="cc-create__row">
        <a
          className="cc-create__btn cc-create__btn--primary"
          href="?panel=communities&step=create"
        >
          Create another
        </a>
      </div>
    </>
  );
}

function NavRow({
  onBack,
  onNext,
  canNext,
}: {
  onBack: () => void;
  onNext: () => void;
  canNext: boolean;
}) {
  return (
    <div className="cc-create__row">
      <button type="button" className="cc-create__btn" onClick={onBack}>
        Back
      </button>
      <button
        type="button"
        className="cc-create__btn cc-create__btn--primary"
        aria-disabled={!canNext}
        disabled={!canNext}
        onClick={onNext}
      >
        Next
      </button>
    </div>
  );
}
