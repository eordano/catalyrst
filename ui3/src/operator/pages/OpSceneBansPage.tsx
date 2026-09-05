import { useMemo, useState } from "react";

import Spinner from "../../atoms/Spinner";
import { ChevronLeft, ChevronRight, Close } from "../../atoms/icons";
import "../../web/pages/stwhatsonadminusers.css";
import "../components/sceneadmins.css";
import "./opscenebanspage.css";
import OpPlacePicker, { type OpPickablePlace } from "../components/OpPlacePicker";

export type OpBanAction = "ban" | "unban";

export type OpBanRow = {
  bannedAddress: string;
  name: string;
};

export type OpScenePlaceRef = {
  id: string;
  title: string | null;
  base_position: string;
  image: string | null;
  user_count: number;
  parcels?: number | null;
  contact_name?: string | null;
};

const ADDR_RE = /^0x[a-fA-F0-9]{40}$/;

function isAddress(value: string | null | undefined): boolean {
  return typeof value === "string" && ADDR_RE.test(value.trim());
}

function shortAddress(value: string): string {
  const v = value.trim();
  return v.length > 12 ? `${v.slice(0, 6)}\u{2026}${v.slice(-4)}` : v;
}

function toPickable(
  p: OpScenePlaceRef,
  banTotal: number,
  selected: boolean,
): OpPickablePlace {
  return {
    id: p.id,
    title: p.title,
    base_position: p.base_position,
    image: p.image,
    user_count: p.user_count,
    moderationHint: selected && banTotal > 0 ? `${banTotal} banned` : null,
  };
}

type BanUserFormProps = {
  onBan: (address: string) => void;
  busy?: boolean;
};

function BanUserForm({ onBan, busy = false }: BanUserFormProps) {
  const [address, setAddress] = useState("");

  const trimmed = address.trim();
  const valid = isAddress(trimmed);
  const showError = trimmed.length > 0 && !valid;

  function submit() {
    if (!valid || busy) return;
    onBan(trimmed);
  }

  return (
    <form
      className="au-field buf"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <label className="au-field__label" htmlFor="buf-wallet">
        Ban a wallet
      </label>
      <div className="buf__row">
        <input
          id="buf-wallet"
          className={"au-field__input" + (showError ? " is-error" : "")}
          placeholder={"0x\u{2026}"}
          value={address}
          onChange={(e) => setAddress(e.target.value)}
          aria-label="Wallet address to ban"
          aria-invalid={showError}
          autoComplete="off"
          spellCheck={false}
          disabled={busy}
        />
        <button
          type="submit"
          className="au-btn au-btn--primary buf__submit"
          disabled={!valid || busy}
        >
          Ban
        </button>
      </div>
      <span className={"au-field__help" + (showError ? " is-error" : "")}>
        {showError ? "Enter a valid Ethereum address (0x + 40 hex)" : " "}
      </span>
    </form>
  );
}

const STEP_LABELS: ReadonlyArray<readonly [string, string]> = [
  ["pick-place", "Pick place"],
  ["bans", "Bans"],
  ["ban-or-unban", "Ban / unban"],
  ["confirm", "Confirm"],
  ["submitting", "Submitting"],
  ["done", "Done"],
];

const ROWS_PER_PAGE = 10;

type SceneBanListProps = {
  rows: OpBanRow[];
  total: number;
  onUnban: (address: string) => void;
  busy?: boolean;
};

function SceneBanList({ rows, total, onUnban, busy = false }: SceneBanListProps) {
  const [page, setPage] = useState(0);

  const paginated = useMemo(
    () => rows.slice(page * ROWS_PER_PAGE, page * ROWS_PER_PAGE + ROWS_PER_PAGE),
    [rows, page],
  );

  const from = rows.length === 0 ? 0 : page * ROWS_PER_PAGE + 1;
  const to = Math.min(rows.length, page * ROWS_PER_PAGE + ROWS_PER_PAGE);
  const lastPage = Math.max(0, Math.ceil(rows.length / ROWS_PER_PAGE) - 1);

  return (
    <div className="au__tablewrap">
      <table className="au__table" aria-label="Banned wallets">
        <thead>
          <tr>
            <th className="au__th">Banned wallet</th>
            <th className="au__th au__th--center">Action</th>
          </tr>
        </thead>
        <tbody>
          {paginated.map((row) => (
            <tr key={row.bannedAddress} className="au-row">
              <td className="au-cell au-cell--user">
                <span className="au-cell__addr">{shortAddress(row.bannedAddress)}</span>
                {row.name ? <span className="au-cell__name">{` (${row.name})`}</span> : null}
              </td>
              <td className="au-cell au-cell--center">
                <button
                  type="button"
                  className="au-btn au-btn--secondary sbl__unban"
                  onClick={() => onUnban(row.bannedAddress)}
                  disabled={busy}
                  aria-label={`Unban ${row.bannedAddress}`}
                >
                  <Close size={16} />
                  Unban
                </button>
              </td>
            </tr>
          ))}
          {paginated.length === 0 && (
            <tr>
              <td className="au-cell au-cell--center au-cell--empty" colSpan={2}>
                No banned wallets for this place
              </td>
            </tr>
          )}
        </tbody>
      </table>

      <div className="au__pagination">
        <span className="au__pgcount">
          {from}&#x2013;{to} of {total}
        </span>
        <button
          type="button"
          className="au__pgbtn"
          aria-label="Go to previous page"
          disabled={page === 0}
          onClick={() => setPage((p) => Math.max(0, p - 1))}
        >
          <ChevronLeft size={20} />
        </button>
        <button
          type="button"
          className="au__pgbtn"
          aria-label="Go to next page"
          disabled={page >= lastPage}
          onClick={() => setPage((p) => Math.min(lastPage, p + 1))}
        >
          <ChevronRight size={20} />
        </button>
      </div>
    </div>
  );
}

export type OpSceneBansPageProps = {
  step: string;
  view: string;
  places: OpScenePlaceRef[];
  owner?: string | null;
  urlPlace: string;
  placeId?: string | null;
  action?: OpBanAction | null;
  address?: string | null;
  error?: string | null;
  result?: { action: OpBanAction; address: string } | null;
  banRows: OpBanRow[];
  banTotal: number;
  onSelectPlace: (id: string) => void;
  onChangePlace: () => void;
  onBan: (address: string) => void;
  onUnban: (address: string) => void;
  onBack: () => void;
  onReview: () => void;
  onSubmit: () => void;
  onReset: () => void;
};

export default function OpSceneBansPage({
  step,
  view,
  places,
  owner,
  urlPlace,
  placeId,
  action,
  address,
  error,
  result,
  banRows,
  banTotal,
  onSelectPlace,
  onChangePlace,
  onBan,
  onUnban,
  onBack,
  onReview,
  onSubmit,
  onReset,
}: OpSceneBansPageProps) {
  const selectedPlace = places.find((p) => p.id === urlPlace) ?? null;

  return (
    <div className="au sb" data-step={step}>
      <div className="au__container sb__container">
        <h1 className="au__title">Scene bans</h1>
        <p className="sb__sub">
          Ban or unban wallets from a place you operate. Confirming performs a{" "}
          <strong>real signed write</strong> &#x2014; connect the operating wallet to
          commit; the endpoint rejects anyone who is not an owner or admin.
        </p>

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

        {(view === "pickPlace" || !selectedPlace) && (
          <OpPlacePicker
            places={places.map((p) => toPickable(p, banTotal, p.id === urlPlace))}
            selectedId={urlPlace || null}
            owner={owner}
            onSelect={onSelectPlace}
          />
        )}

        {selectedPlace && view !== "pickPlace" && (
          <p className="sb__place">
            Moderating <strong>{selectedPlace.title || selectedPlace.id}</strong>{" "}
            <span className="sb__coords">{selectedPlace.base_position}</span>
            <button type="button" className="sb__change" onClick={onChangePlace}>
              Change place
            </button>
          </p>
        )}

        {view === "bans" && selectedPlace && (
          <>
            <BanUserForm onBan={onBan} />
            <SceneBanList
              rows={banRows}
              total={banTotal}
              onUnban={onUnban}
            />
          </>
        )}

        {view === "banOrUnban" && (
          <div className="sb__stage">
            <p className="sb__staged">
              {action === "unban" ? "Unban" : "Ban"}{" "}
              <code className="sb__addr">{shortAddress(address ?? "")}</code>
            </p>
            <div className="sb__controls" role="group" aria-label="Review action">
              <button
                type="button"
                className="au-btn au-btn--secondary"
                onClick={onBack}
              >
                Back
              </button>
              <button
                type="button"
                className="au-btn au-btn--primary"
                onClick={onReview}
              >
                Review
              </button>
            </div>
          </div>
        )}

        {view === "confirm" && (
          <div className="sb__confirm">
            <h2 className="sb__confirm-h">
              {action === "unban" ? "Unban wallet" : "Ban wallet"}
            </h2>
            <dl className="sb__summary">
              <div>
                <dt>Action</dt>
                <dd>{action === "unban" ? "Unban" : "Ban"}</dd>
              </div>
              <div>
                <dt>Wallet</dt>
                <dd>
                  <code className="sb__addr">{address}</code>
                </dd>
              </div>
              <div>
                <dt>Place</dt>
                <dd>{selectedPlace?.title || placeId}</dd>
              </div>
            </dl>
            {error ? (
              <div className="au-alert au-alert--error sb__alert" role="alert">
                <span className="au-alert__msg">Commit failed: {error}. Retry?</span>
              </div>
            ) : null}
            <div className="sb__controls" role="group" aria-label="Confirm action">
              <button
                type="button"
                className="au-btn au-btn--secondary"
                onClick={onBack}
              >
                Back
              </button>
              <button
                type="button"
                className="au-btn au-btn--primary"
                onClick={onSubmit}
              >
                {error ? "Retry" : action === "unban" ? "Confirm unban" : "Confirm ban"}
              </button>
            </div>
          </div>
        )}

        {view === "submitting" && (
          <div className="sb__submitting" role="status" aria-live="polite">
            <Spinner size={18} color="var(--au-grad-a, #a042cd)" aria-hidden />
            <span>
              {action === "unban" ? "Unbanning" : "Banning"}{" "}
              {shortAddress(address ?? "")}&#x2026;
            </span>
          </div>
        )}

        {view === "done" && (
          <div className="sb__done">
            <div className="au-alert au-alert--success sb__alert" role="status">
              <span className="au-alert__msg">
                {result?.action === "unban" ? "Unbanned" : "Banned"}{" "}
                {shortAddress(result?.address ?? "")} for{" "}
                {selectedPlace?.title || placeId}.
              </span>
            </div>
            <button
              type="button"
              className="au-btn au-btn--primary"
              onClick={onReset}
            >
              Back to ban list
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
