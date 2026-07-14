import Spinner from "../../atoms/Spinner";
import "../../web/pages/stwhatsonadminusers.css";
import "../components/sceneadmins.css";
import OpPlacePicker, { type OpPickablePlace } from "../components/OpPlacePicker";
import { isValidAddress, truncateAddress } from "../../data/format";

export type OpAdminAction = "add" | "revoke";

export type OpSceneAdminRow = {
  id: string | null;
  admin: string;
  name: string;
  canBeRemoved: boolean;
  added_by: string | null;
  created_at: number | null;
  place_id?: string | null;
  active?: boolean | null;
};

export type OpOperatedPlace = {
  id: string;
  title: string;
  base_position: string;
  world: boolean;
  world_name: string | null;
  image: string | null;
  positions?: string[];
  owner?: string | null;
};

type OpGrantKind = "explicit" | "implicit";

type OpAdminEntry = {
  admin: string;
  name: string;
  kind: OpGrantKind;
  canBeRemoved: boolean;
  addedBy: string | null;
  createdAt: number | null;
};

function rowKind(row: OpSceneAdminRow): OpGrantKind {
  return row.id ? "explicit" : "implicit";
}

function toAdminEntry(row: OpSceneAdminRow): OpAdminEntry {
  return {
    admin: row.admin.toLowerCase(),
    name: row.name,
    kind: rowKind(row),
    canBeRemoved: row.canBeRemoved,
    addedBy: row.added_by,
    createdAt: row.created_at,
  };
}

function partitionGrants(rows: OpSceneAdminRow[]): {
  explicit: OpAdminEntry[];
  implicit: OpAdminEntry[];
} {
  const explicit: OpAdminEntry[] = [];
  const implicit: OpAdminEntry[] = [];
  for (const row of rows) {
    const entry = toAdminEntry(row);
    (entry.kind === "explicit" ? explicit : implicit).push(entry);
  }
  return { explicit, implicit };
}


function placeLabel(p: OpOperatedPlace): string {
  if (p.title) return p.title;
  if (p.world && p.world_name) return p.world_name;
  return p.base_position;
}

function toPickable(p: OpOperatedPlace): OpPickablePlace {
  return {
    id: p.id,
    title: p.title,
    base_position: p.world && p.world_name ? p.world_name : p.base_position,
    image: p.image,
    user_count: null,
  };
}

function NameCell({ entry }: { entry: OpAdminEntry }) {
  return (
    <td className="au-cell au-cell--user">
      <span className="au-cell__name sa-cell__name--strong">
        {entry.name || "\u{2014}"}
      </span>
      <span className="au-cell__addr sa-cell__addr--gap">
        {truncateAddress(entry.admin)}
      </span>
    </td>
  );
}

type SceneAdminListProps = {
  grants: OpSceneAdminRow[];
  onRevoke: (admin: string, canBeRemoved: boolean) => void;
};

function SceneAdminList({ grants, onRevoke }: SceneAdminListProps) {
  const { explicit, implicit } = partitionGrants(grants);

  return (
    <div className="sa-list">
      <section className="sa-list__section">
        <h3 className="sa-list__heading">
          Explicit admins <span className="sa-list__count">{explicit.length}</span>
        </h3>
        <div className="au__tablewrap">
          <table className="au__table">
            <thead>
              <tr>
                <th className="au-cell">Admin</th>
                <th className="au-cell">Granted by</th>
                <th className="au-cell au-cell--center sa-list__action-col">
                  Action
                </th>
              </tr>
            </thead>
            <tbody>
              {explicit.length === 0 ? (
                <tr>
                  <td className="au-cell au-cell--empty" colSpan={3}>
                    No explicit scene admins for this place.
                  </td>
                </tr>
              ) : (
                explicit.map((e) => (
                  <tr className="au-row" key={e.admin}>
                    <NameCell entry={e} />
                    <td className="au-cell au-cell__addr">
                      {e.addedBy ? truncateAddress(e.addedBy) : "\u{2014}"}
                    </td>
                    <td className="au-cell au-cell--center">
                      <button
                        type="button"
                        className="sa-btn sa-btn--danger"
                        disabled={!e.canBeRemoved}
                        title={
                          e.canBeRemoved
                            ? "Revoke this admin grant"
                            : "Cannot revoke: this address is also an inherited (extra) grant"
                        }
                        onClick={() => onRevoke(e.admin, e.canBeRemoved)}
                      >
                        Revoke
                      </button>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>

      <section className="sa-list__section">
        <h3 className="sa-list__heading">
          Inherited grants <span className="sa-list__count">{implicit.length}</span>
        </h3>
        <p className="sa-list__note">
          Extra-address and land-lease grants. Shown for context &#x2014; managed
          elsewhere (collection / world managers, land leases) and not removable
          here.
        </p>
        <div className="au__tablewrap">
          <table className="au__table">
            <thead>
              <tr>
                <th className="au-cell">Address</th>
                <th className="au-cell au-cell--center sa-list__action-col">
                  Removable
                </th>
              </tr>
            </thead>
            <tbody>
              {implicit.length === 0 ? (
                <tr>
                  <td className="au-cell au-cell--empty" colSpan={2}>
                    No inherited grants.
                  </td>
                </tr>
              ) : (
                implicit.map((e) => (
                  <tr className="au-row" key={e.admin}>
                    <NameCell entry={e} />
                    <td className="au-cell au-cell--center">
                      <span className="sa-tag">No</span>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}

type GrantAdminFormProps = {
  value: string;
  onChange: (next: string) => void;
  onReview: () => void;
  onCancel: () => void;
};

function GrantAdminForm({ value, onChange, onReview, onCancel }: GrantAdminFormProps) {
  const trimmed = value.trim();
  const valid = isValidAddress(trimmed);
  const showError = trimmed.length > 0 && !valid;

  return (
    <form
      className="sa-form"
      onSubmit={(e) => {
        e.preventDefault();
        if (valid) onReview();
      }}
    >
      <div className={"au-field" + (showError ? " is-error" : "")}>
        <label className="au-field__label" htmlFor="sa-admin">
          Wallet address to grant admin
        </label>
        <input
          id="sa-admin"
          className={"au-field__input" + (showError ? " is-error" : "")}
          type="text"
          inputMode="text"
          autoComplete="off"
          spellCheck={false}
          placeholder="0x0000000000000000000000000000000000000000"
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
        <span className={"au-field__help" + (showError ? " is-error" : "")}>
          {showError
            ? "Enter a valid 0x address (40 hex characters)."
            : "The address that may administer this scene."}
        </span>
      </div>

      <div className="sa-form__actions">
        <button type="button" className="sa-btn sa-btn--ghost" onClick={onCancel}>
          Back
        </button>
        <button
          type="submit"
          className="sa-btn sa-btn--primary"
          disabled={!valid}
        >
          Review grant
        </button>
      </div>
    </form>
  );
}

const STEP_LABELS: ReadonlyArray<readonly [string, string]> = [
  ["pick-place", "Pick place"],
  ["admins", "Admins"],
  ["grant-or-revoke", "Grant / revoke"],
  ["confirm", "Confirm"],
  ["submitting", "Submitting"],
  ["done", "Done"],
];

function RevokeSummary({
  admin,
  grants,
  onReview,
  onCancel,
}: {
  admin: string;
  grants: OpSceneAdminRow[];
  onReview: () => void;
  onCancel: () => void;
}) {
  const { explicit } = partitionGrants(grants);
  const entry = explicit.find((e) => e.admin === admin);
  return (
    <div className="sa-revoke">
      <h2 className="sa-revoke__title">Revoke this admin?</h2>
      <p className="sa-revoke__row">
        <span className="au-cell__name sa-cell__name--strong">
          {entry?.name || "\u{2014}"}
        </span>
        <span className="au-cell__addr sa-cell__addr--gap">
          {truncateAddress(admin)}
        </span>
      </p>
      <div className="sa-form__actions">
        <button type="button" className="sa-btn sa-btn--ghost" onClick={onCancel}>
          Back
        </button>
        <button type="button" className="sa-btn sa-btn--danger" onClick={onReview}>
          Review revoke
        </button>
      </div>
    </div>
  );
}

export type OpManageAdminsViewProps = {
  step: string;
  view: string;
  places: OpOperatedPlace[];
  grants: OpSceneAdminRow[];
  activePlaceId: string | null;
  action?: OpAdminAction | null;
  grantAddress?: string | null;
  revokeAddress?: string | null;
  error?: string | null;
  onSelectPlace: (placeId: string) => void;
  onStartGrant: (address: string) => void;
  onStartRevoke: (admin: string, canBeRemoved: boolean) => void;
  onReview: () => void;
  onBack: () => void;
  onSubmit: () => void;
  onDoneBack: () => void;
};

export default function OpManageAdminsView({
  step,
  view,
  places,
  grants,
  activePlaceId,
  action,
  grantAddress,
  revokeAddress,
  error,
  onSelectPlace,
  onStartGrant,
  onStartRevoke,
  onReview,
  onBack,
  onSubmit,
  onDoneBack,
}: OpManageAdminsViewProps) {
  const activePlace = places.find((p) => p.id === activePlaceId) ?? null;

  const placeName = activePlace ? placeLabel(activePlace) : (activePlaceId ?? "\u{2014}");

  const targetAddress =
    action === "revoke" ? revokeAddress ?? "" : grantAddress ?? "";

  return (
    <div className="sa" data-step={step}>
      <header className="sa__head">
        <h1 className="sa__title">Manage scene admins</h1>
        <p className="sa__sub">
          Grant or revoke who may administer a place you operate. Confirming
          performs a <strong>real signed write</strong> &#x2014; connect the operating
          wallet to commit; the endpoint rejects non-owners and non-admins.
        </p>
        {view !== "pickPlace" && activePlace && (
          <div className="sa__context">
            <span className="sa__context-label">Editing</span>
            <span className="sa__context-place">{placeName}</span>
            <span className="sa__context-coords">
              {activePlace.world && activePlace.world_name
                ? activePlace.world_name
                : activePlace.base_position}
            </span>
          </div>
        )}
      </header>

      <ol className="sa__steps" aria-label="Steps">
        {STEP_LABELS.map(([slug, label]) => (
          <li
            key={slug}
            className={"sa__step" + (slug === step ? " is-active" : "")}
            aria-current={slug === step ? "step" : undefined}
          >
            {label}
          </li>
        ))}
      </ol>

      <div className="sa__body">
        {view === "pickPlace" && (
          <OpPlacePicker
            places={places.map(toPickable)}
            selectedId={activePlaceId}
            owner={null}
            onSelect={onSelectPlace}
          />
        )}

        {view === "admins" && (
          <>
            <div className="sa__toolbar">
              <OpPlacePicker
                places={places.map(toPickable)}
                selectedId={activePlaceId}
                compact
                onSelect={onSelectPlace}
              />
              <button
                type="button"
                className="sa-btn sa-btn--primary"
                onClick={() => onStartGrant("")}
              >
                + Grant admin
              </button>
            </div>
            <SceneAdminList
              grants={grants}
              onRevoke={onStartRevoke}
            />
          </>
        )}

        {view === "grantOrRevoke" && action === "add" && (
          <GrantAdminForm
            value={grantAddress ?? ""}
            onChange={onStartGrant}
            onReview={onReview}
            onCancel={onBack}
          />
        )}
        {view === "grantOrRevoke" && action === "revoke" && (
          <RevokeSummary
            admin={revokeAddress ?? ""}
            grants={grants}
            onReview={onReview}
            onCancel={onBack}
          />
        )}

        {view === "confirm" && (
          <div className="sa-confirm">
            <h2 className="sa-confirm__title">
              Confirm {action === "revoke" ? "revoke" : "grant"}
            </h2>
            <dl className="sa-confirm__grid">
              <dt>Action</dt>
              <dd>{action === "revoke" ? "Revoke admin" : "Grant admin"}</dd>
              <dt>Place</dt>
              <dd>{placeName}</dd>
              <dt>Address</dt>
              <dd className="sa-confirm__addr">{targetAddress}</dd>
            </dl>
            <p className="sa-confirm__sim">
              {action === "revoke"
                ? "Signs DELETE /scene-admin?place_id=\u{2026}&admin=\u{2026}"
                : "Signs POST /scene-admin { place_id, admin }"}{" "}
              &#x2014; is_scene_owner_or_admin gated (403 for an anonymous browser or a
              non-operator wallet).
            </p>
            {error && (
              <p className="au-alert au-alert--error sa-confirm__error" role="alert">
                Commit failed: {error}
              </p>
            )}
            <div className="sa-form__actions">
              <button
                type="button"
                className="sa-btn sa-btn--ghost"
                onClick={onBack}
              >
                Back
              </button>
              <button
                type="button"
                className="sa-btn sa-btn--primary"
                onClick={onSubmit}
              >
                {error ? "Retry" : "Confirm"}
              </button>
            </div>
          </div>
        )}

        {view === "submitting" && (
          <div
            className="sa-progress"
            role="status"
            aria-live="polite"
            aria-busy="true"
          >
            <Spinner size={36} aria-hidden />
            <p className="sa-progress__label">
              {action === "revoke" ? "Revoking admin\u{2026}" : "Granting admin\u{2026}"}
            </p>
          </div>
        )}

        {view === "done" && (
          <div className="sa-done" role="status">
            <div className="sa-done__check au-check" aria-hidden="true">
              &#x2713;
            </div>
            <h2 className="sa-done__title">
              {action === "revoke" ? "Admin revoked" : "Admin granted"}
            </h2>
            <p className="sa-done__sub">
              {truncateAddress(targetAddress)} on {placeName}.
            </p>
            <div className="sa-form__actions">
              <button
                type="button"
                className="sa-btn sa-btn--primary"
                onClick={onDoneBack}
              >
                Back to admin list
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
