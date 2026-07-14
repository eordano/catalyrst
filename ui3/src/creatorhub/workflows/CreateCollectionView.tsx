import { useState } from "react";
import type { ComponentType, DragEvent, ReactNode } from "react";

import Spinner from "../../atoms/Spinner";
import Button from "../../atoms/Button";
import "./createcollectionview.css";

const STEP_SLUGS = ["name", "items", "review", "submit", "done"] as const;

type CwcDraftItem = {
  id: string;
  name: string;
  size: number;
  fileType: string;
  thumbnail?: string;
};

type LinkComponentProps = {
  className?: string;
  to: string;
  children?: ReactNode;
};

type CreateCollectionViewProps = {
  view?: string;
  step?: string;
  name?: string;
  type?: "standard" | "linked";
  items?: CwcDraftItem[];
  uploads?: CwcDraftItem[];
  skippedFiles?: string[];
  accept?: string[];
  nameSuggestions?: string[];
  feePerItem?: number;
  nameMax?: number;
  cost?: number;
  error?: string;
  collectionHref?: string;
  thirdPartyHref?: string;
  standardHref?: string;
  LinkComponent?: ComponentType<LinkComponentProps>;
  onSubmitName?: (name: string) => void;
  onFilesSelected?: (files: File[]) => void;
  onRemoveUpload?: (id: string) => void;
  onAddItems?: () => void;
  onSubmit?: () => void;
  onBack?: () => void;
  onRetry?: () => void;
};

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export default function CreateCollectionView({
  view = "naming",
  step = "name",
  name = "",
  type = "standard",
  items = [],
  uploads = [],
  skippedFiles = [],
  accept = [".zip", ".gltf", ".glb", ".png"],
  nameSuggestions = [],
  feePerItem = 100,
  nameMax = 32,
  cost = 0,
  error = undefined,
  collectionHref = "#",
  thirdPartyHref = "?type=linked",
  standardHref = "?type=standard",
  LinkComponent = undefined,
  onSubmitName = undefined,
  onFilesSelected = undefined,
  onRemoveUpload = undefined,
  onAddItems = undefined,
  onSubmit = undefined,
  onBack = undefined,
  onRetry = undefined,
}: CreateCollectionViewProps) {
  const [nameDraft, setNameDraft] = useState(name || "");
  const [nameTouched, setNameTouched] = useState(false);
  const [dragging, setDragging] = useState(false);

  const isValidName = (n: string) => {
    const t = n.trim();
    return t.length >= 1 && t.length <= nameMax;
  };

  const QuietLink = ({ to, children }: { to: string; children: ReactNode }) =>
    LinkComponent ? (
      <LinkComponent className="cwc-create-collection__quietlink" to={to}>
        {children}
      </LinkComponent>
    ) : (
      <a className="cwc-create-collection__quietlink" href={to}>
        {children}
      </a>
    );

  const handleDrop = (e: DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    setDragging(false);
    const files = Array.from(e.dataTransfer?.files ?? []);
    if (files.length) onFilesSelected?.(files);
  };

  return (
    <div className="cwc-create-collection" data-step={step}>
        <header className="cwc-create-collection__head">
          <h1 className="cwc-create-collection__title">Create a Collection</h1>
          <ol className="cwc-create-collection__steps" aria-label="Wizard steps">
            {STEP_SLUGS.map((s, i) => (
              <li
                key={s}
                className={
                  "cwc-create-collection__stepdot" +
                  (s === step ? " is-active" : "")
                }
                aria-current={s === step ? "step" : undefined}
              >
                <span className="cwc-create-collection__stepnum">{i + 1}</span>
                <span className="cwc-create-collection__steplabel">{s}</span>
              </li>
            ))}
          </ol>
        </header>

        {view === "naming" && (
          <section className="cwc-create-collection__panel" aria-label="Name your collection">
            {type === "linked" && (
              <p className="cwc-create-collection__typenote">
                Third-party (linked) collection &#x2014; registered by a Third Party
                Provider, no per-item publication fee.
              </p>
            )}
            <label className="cwc-create-collection__label" htmlFor="cc-name">
              Collection name
            </label>
            <input
              id="cc-name"
              className="cwc-create-collection__input"
              type="text"
              maxLength={nameMax}
              placeholder={nameSuggestions[0] ?? "My Collection"}
              value={nameDraft}
              onChange={(e) => setNameDraft(e.target.value)}
              onBlur={() => setNameTouched(true)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && isValidName(nameDraft)) onSubmitName?.(nameDraft);
              }}
              aria-invalid={nameTouched && !isValidName(nameDraft)}
            />
            <p className="cwc-create-collection__error" role="alert">
              {nameTouched && !isValidName(nameDraft)
                ? `Enter a name between 1 and ${nameMax} characters.`
                : null}
            </p>
            <div className="cwc-create-collection__controls">
              <Button
                variant="primary"
                disabled={!isValidName(nameDraft)}
                onClick={() => onSubmitName?.(nameDraft)}
              >
                Continue
              </Button>
            </div>
            <p className="cwc-create-collection__footnote">
              {type === "linked" ? (
                <QuietLink to={standardHref}>
                  Create a standard collection instead?
                </QuietLink>
              ) : (
                <QuietLink to={thirdPartyHref}>
                  Managing a registered third-party collection?
                </QuietLink>
              )}
            </p>
          </section>
        )}

        {view === "editingItems" && (
          <section className="cwc-create-collection__panel" aria-label="Upload items">
            <div
              className={
                "cwc-create-collection__dropzone" + (dragging ? " is-dragging" : "")
              }
              onDragOver={(e) => {
                e.preventDefault();
                setDragging(true);
              }}
              onDragLeave={() => setDragging(false)}
              onDrop={handleDrop}
            >
              <p className="cwc-create-collection__droptitle">
                Drag & drop your wearable or emote files here
              </p>
              <p className="cwc-create-collection__drophint">
                Supported: {accept.join(", ")}
              </p>
              <p className="cwc-create-collection__drophint">
                Files are previewed locally only &#x2014; nothing is uploaded or
                validated yet.
              </p>
              <label
                className="cwc-create-collection__btn cwc-create-collection__filebtn"
                htmlFor="cc-files"
              >
                Browse files
              </label>
              <input
                id="cc-files"
                className="cwc-create-collection__fileinput"
                type="file"
                multiple
                accept={accept.join(",")}
                onChange={(e) => {
                  const files = Array.from(e.target.files ?? []);
                  if (files.length) onFilesSelected?.(files);
                  e.target.value = "";
                }}
              />
            </div>

            {skippedFiles.length > 0 && (
              <p className="cwc-create-collection__skipped" role="alert">
                Skipped unsupported file(s): {skippedFiles.join(", ")}
              </p>
            )}

            {uploads.length > 0 && (
              <ul className="cwc-create-collection__uploadlist" aria-label="Added files">
                {uploads.map((u) => (
                  <li key={u.id} className="cwc-create-collection__upload">
                    {u.thumbnail ? (
                      <img
                        className="cwc-create-collection__uploadthumb"
                        src={u.thumbnail}
                        alt=""
                      />
                    ) : (
                      <span className="cwc-create-collection__uploadext">
                        {u.fileType}
                      </span>
                    )}
                    <span className="cwc-create-collection__uploadname">{u.name}</span>
                    <span className="cwc-create-collection__uploadmeta">
                      {u.fileType} &#xB7; {formatSize(u.size)}
                    </span>
                    <button
                      type="button"
                      className="cwc-create-collection__uploadremove"
                      aria-label={`Remove ${u.name}`}
                      onClick={() => onRemoveUpload?.(u.id)}
                    >
                      &#xD7;
                    </button>
                  </li>
                ))}
              </ul>
            )}

            <div className="cwc-create-collection__controls">
              <Button variant="secondary" onClick={() => onBack?.()}>
                Back
              </Button>
              <Button
                variant="primary"
                disabled={uploads.length === 0}
                onClick={() => onAddItems?.()}
              >
                Review {uploads.length} item{uploads.length === 1 ? "" : "s"}
              </Button>
            </div>
          </section>
        )}

        {view === "reviewing" && (
          <section className="cwc-create-collection__panel" aria-label="Review">
            <h2 className="cwc-create-collection__subtitle">{name}</h2>
            <p className="cwc-create-collection__typebadge">
              {type === "linked" ? "Linked Wearables" : "Standard Collection"}
            </p>
            <ul className="cwc-create-collection__itemlist">
              {items.map((it) => (
                <li key={it.id} className="cwc-create-collection__item">
                  <span className="cwc-create-collection__itemname">{it.name}</span>
                  <span className="cwc-create-collection__itemcat">{it.fileType}</span>
                  <span className="cwc-create-collection__itemsize">
                    {formatSize(it.size)}
                  </span>
                </li>
              ))}
            </ul>
            <div className="cwc-create-collection__cost">
              <span>Estimated publish cost</span>
              <strong>
                {cost.toLocaleString()} MANA
                {type === "linked" && " (no per-item fee)"}
              </strong>
              <small>
                {items.length} item(s) &#xD7; {feePerItem} MANA / item &#x2014; quote
                is simulated (read on-chain at publish time).
              </small>
            </div>
            <div className="cwc-create-collection__controls">
              <Button variant="secondary" onClick={() => onBack?.()}>
                Back
              </Button>
              <Button variant="primary" onClick={() => onSubmit?.()}>
                Sign & submit
              </Button>
            </div>
          </section>
        )}

        {view === "submitting" && (
          <section className="cwc-create-collection__panel cwc-create-collection__panel--progress" aria-label="Submitting">
            <div style={{ marginBottom: 20 }}>
              <Spinner size={36} aria-hidden />
            </div>
            <p className="cwc-create-collection__progress">
              Signing & submitting the collection contract&#x2026;
            </p>
            <small className="cwc-create-collection__note">
              On-chain mint is SIMULATED &#x2014; no wallet/signer is invoked.
            </small>
          </section>
        )}

        {view === "done" && (
          <section className="cwc-create-collection__panel cwc-create-collection__panel--done" aria-label="Created">
            <h2 className="cwc-create-collection__subtitle">Collection created (simulated)</h2>
            <p className="cwc-create-collection__done">
              "{name}" was submitted with {items.length} item
              {items.length === 1 ? "" : "s"}. The on-chain mint is simulated
              and your items are kept locally in this browser.
            </p>
            <a
              className="cwc-create-collection__btn cwc-create-collection__btn--primary"
              href={collectionHref}
            >
              Open collection
            </a>
          </section>
        )}

        {view === "error" && (
          <section className="cwc-create-collection__panel" aria-label="Error">
            <p className="cwc-create-collection__error" role="alert">
              Could not submit the collection: {error}
            </p>
            <div className="cwc-create-collection__controls">
              <Button variant="primary" onClick={() => onRetry?.()}>
                Retry
              </Button>
            </div>
          </section>
        )}
    </div>
  );
}
