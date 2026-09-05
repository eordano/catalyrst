import { useEffect, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import GovernanceChrome from "@ui/governance/frames/GovernanceChrome";
import "@ui/governance/pages/gvsubmitlinkedwearables.css";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  type LinkedWearablesData,
  type LinkedWearablesSample,
  MAX_IMAGES,
} from "@data/lib/catalyst/governance/submit-linked-wearables";
import {
  submitLwMachine,
  resolveLwSnapshot,
  slugToState,
  stateToSlug,
  type CreateFn,
  type TrackFn,
} from "./machine";

export type GvSubmitLinkedWearablesWizardProps = {
  trackCtx: TrackContext;
  data: LinkedWearablesData;
  initialStep?: string;
  create?: CreateFn;
  track?: TrackFn;
};

function ListField({
  values,
  placeholder,
  addLabel,
  onAdd,
  onEdit,
  onRemove,
  canAdd = true,
}: {
  values: string[];
  placeholder: string;
  addLabel: string;
  onAdd: () => void;
  onEdit: (i: number, value: string) => void;
  onRemove: (i: number) => void;
  canAdd?: boolean;
}) {
  return (
    <div className="gvsubmitlinkedwearables__list">
      {values.map((value, i) => (
        <div key={i} className="gvsubmitlinkedwearables__listrow">
          <input
            className="gvsubmitlinkedwearables__input"
            placeholder={placeholder}
            value={value}
            onChange={(e) => onEdit(i, e.target.value)}
          />
          <button
            type="button"
            className="gvsubmitlinkedwearables__remove"
            aria-label="Remove"
            onClick={() => onRemove(i)}
          >
            &#xD7;
          </button>
        </div>
      ))}
      {canAdd && (
        <button type="button" className="gvsubmitlinkedwearables__add" onClick={onAdd}>
          {addLabel}
        </button>
      )}
    </div>
  );
}

export default function GvSubmitLinkedWearablesWizard(
  props: GvSubmitLinkedWearablesWizardProps,
) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return <Inner key={stateId} stateId={stateId} {...props} />;
}

type InnerProps = GvSubmitLinkedWearablesWizardProps & {
  stateId: ReturnType<typeof slugToState>;
};

function Inner({ stateId, trackCtx, data, create, track }: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const sample: LinkedWearablesSample = data.sample;

  const seeded = stateId !== "identity" && stateId !== "collection";
  const snapshot = useRef(
    resolveLwSnapshot({
      step: stateId,
      trackCtx,
      create,
      track,
      identity: seeded
        ? { name: sample.name, marketplaceLink: sample.marketplace_link, links: sample.links }
        : undefined,
      collection:
        stateId === "technical" || stateId === "review" || stateId === "submitting" || stateId === "error"
          ? { imagePreviews: sample.image_previews, nftCollections: sample.nft_collections, items: sample.items }
          : undefined,
      technical:
        stateId === "review" || stateId === "submitting" || stateId === "error"
          ? {
              smartContracts: sample.smart_contract,
              managers: sample.managers,
              programmaticallyGenerated: sample.programmatically_generated,
              method: sample.method,
            }
          : undefined,
      coAuthors: seeded ? sample.coAuthors : undefined,
    }),
  ).current;

  const [state, send] = useMachine(submitLwMachine, {
    input: { trackCtx, create, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const errors = state.context.errors;

  const ctx = state.context;
  const [name, setName] = useState(ctx.identity.name);
  const [marketplaceLink, setMarketplaceLink] = useState(ctx.identity.marketplaceLink);
  const [links, setLinks] = useState<string[]>(ctx.identity.links.length ? ctx.identity.links : [""]);
  const [images, setImages] = useState<string[]>(
    ctx.collection.imagePreviews.length ? ctx.collection.imagePreviews : [""],
  );
  const [nftCollections, setNftCollections] = useState(ctx.collection.nftCollections);
  const [items, setItems] = useState(ctx.collection.items);
  const [contracts, setContracts] = useState<string[]>(
    ctx.technical.smartContracts.length ? ctx.technical.smartContracts : [""],
  );
  const [managers, setManagers] = useState<string[]>(
    ctx.technical.managers.length ? ctx.technical.managers : [""],
  );
  const [programmatic, setProgrammatic] = useState(ctx.technical.programmaticallyGenerated);
  const [method, setMethod] = useState(ctx.technical.method);
  const [coAuthor, setCoAuthor] = useState(ctx.coAuthors[0] ?? "");

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

  const editAt = (setter: React.Dispatch<React.SetStateAction<string[]>>) => (i: number, v: string) =>
    setter((arr) => arr.map((x, idx) => (idx === i ? v : x)));
  const addTo = (setter: React.Dispatch<React.SetStateAction<string[]>>) => () =>
    setter((arr) => [...arr, ""]);
  const removeAt = (setter: React.Dispatch<React.SetStateAction<string[]>>) => (i: number) =>
    setter((arr) => {
      const next = arr.filter((_, idx) => idx !== i);
      return next.length ? next : [""];
    });

  const f = data.fields;

  return (
    <GovernanceChrome active="proposals" onTab={() => {}}>
      <div className="gvsubmitlinkedwearables" data-step={step}>
        <div className="gvsubmitlinkedwearables__layout">
          <div className="gvsubmitlinkedwearables__container">
            <section className="gvsubmitlinkedwearables__section">
              <h1 className="gvsubmitlinkedwearables__h1">{data.title}</h1>
            </section>

            {value === "identity" && (
              <form
                className="gvsubmitlinkedwearables__form"
                onSubmit={(e) => {
                  e.preventDefault();
                  send({ type: "FILL_IDENTITY", identity: { name, marketplaceLink, links } });
                }}
              >
                <section className="gvsubmitlinkedwearables__section">
                  <p className="gvsubmitlinkedwearables__desc">{data.description}</p>
                  <p className="gvsubmitlinkedwearables__desc">{data.description_note}</p>
                </section>

                <section className="gvsubmitlinkedwearables__section">
                  <div className="gvsubmitlinkedwearables__label">{f.name.label}</div>
                  <p className="gvsubmitlinkedwearables__sublabel">{f.name.detail}</p>
                  <input
                    className="gvsubmitlinkedwearables__input"
                    placeholder={f.name.placeholder ?? ""}
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                  />
                  <div className="gvsubmitlinkedwearables__counter">
                    {name.length}/{f.name.max_length ?? 80}
                  </div>
                  {errors.name && <p className="gvsubmitlinkedwearables__error">{errors.name}</p>}
                </section>

                <section className="gvsubmitlinkedwearables__section">
                  <div className="gvsubmitlinkedwearables__label">{f.marketplace_link.label}</div>
                  <p className="gvsubmitlinkedwearables__sublabel">{f.marketplace_link.detail}</p>
                  <input
                    className="gvsubmitlinkedwearables__input"
                    placeholder={f.marketplace_link.placeholder ?? ""}
                    value={marketplaceLink}
                    onChange={(e) => setMarketplaceLink(e.target.value)}
                  />
                  {errors.marketplace_link && (
                    <p className="gvsubmitlinkedwearables__error">{errors.marketplace_link}</p>
                  )}
                </section>

                <section className="gvsubmitlinkedwearables__section">
                  <div className="gvsubmitlinkedwearables__label">{f.links.label}</div>
                  <p className="gvsubmitlinkedwearables__sublabel">{f.links.detail}</p>
                  <ListField
                    values={links}
                    placeholder={f.links.placeholder ?? "Add a link here"}
                    addLabel={f.links.add_label ?? "Add another link"}
                    onAdd={addTo(setLinks)}
                    onEdit={editAt(setLinks)}
                    onRemove={removeAt(setLinks)}
                  />
                  {errors.links && <p className="gvsubmitlinkedwearables__error">{errors.links}</p>}
                </section>

                <section className="gvsubmitlinkedwearables__section">
                  <button type="submit" className="gvsubmitlinkedwearables__submit">
                    Continue
                  </button>
                </section>
              </form>
            )}

            {value === "collection" && (
              <form
                className="gvsubmitlinkedwearables__form"
                onSubmit={(e) => {
                  e.preventDefault();
                  send({
                    type: "FILL_COLLECTION",
                    collection: { imagePreviews: images, nftCollections, items },
                  });
                }}
              >
                <section className="gvsubmitlinkedwearables__section">
                  <div className="gvsubmitlinkedwearables__label">{f.image_previews.label}</div>
                  <p className="gvsubmitlinkedwearables__sublabel">
                    {f.image_previews.detail.replace("{amount}", String(MAX_IMAGES))}
                  </p>
                  <ListField
                    values={images}
                    placeholder={f.image_previews.placeholder ?? "Insert image URL"}
                    addLabel={f.image_previews.add_label ?? "Add another image"}
                    onAdd={addTo(setImages)}
                    onEdit={editAt(setImages)}
                    onRemove={removeAt(setImages)}
                    canAdd={images.length < MAX_IMAGES}
                  />
                  {errors.image_previews && (
                    <p className="gvsubmitlinkedwearables__error">{errors.image_previews}</p>
                  )}
                </section>

                <section className="gvsubmitlinkedwearables__section">
                  <div className="gvsubmitlinkedwearables__label">
                    {f.nft_collections.label}
                    <span className="gvsubmitlinkedwearables__mdnotice">Markdown supported</span>
                  </div>
                  <p className="gvsubmitlinkedwearables__sublabel">{f.nft_collections.detail}</p>
                  <textarea
                    className="gvsubmitlinkedwearables__textarea"
                    placeholder={f.nft_collections.placeholder ?? ""}
                    value={nftCollections}
                    onChange={(e) => setNftCollections(e.target.value)}
                  />
                  <div className="gvsubmitlinkedwearables__counter">
                    {nftCollections.length}/{f.nft_collections.max_length ?? 750}
                  </div>
                  {errors.nft_collections && (
                    <p className="gvsubmitlinkedwearables__error">{errors.nft_collections}</p>
                  )}
                </section>

                <section className="gvsubmitlinkedwearables__section">
                  <div className="gvsubmitlinkedwearables__label">{f.items.label}</div>
                  <p className="gvsubmitlinkedwearables__sublabel">{f.items.detail}</p>
                  <input
                    className="gvsubmitlinkedwearables__input gvsubmitlinkedwearables__input--number"
                    type="number"
                    min={f.items.minimum}
                    placeholder={f.items.placeholder ?? ""}
                    value={items}
                    onChange={(e) => setItems(e.target.value)}
                  />
                  {errors.items && <p className="gvsubmitlinkedwearables__error">{errors.items}</p>}
                </section>

                <section className="gvsubmitlinkedwearables__section gvsubmitlinkedwearables__row">
                  <button
                    type="button"
                    className="gvsubmitlinkedwearables__add"
                    onClick={() => send({ type: "BACK" })}
                  >
                    Back
                  </button>
                  <button type="submit" className="gvsubmitlinkedwearables__submit">
                    Continue
                  </button>
                </section>
              </form>
            )}

            {value === "technical" && (
              <form
                className="gvsubmitlinkedwearables__form"
                onSubmit={(e) => {
                  e.preventDefault();
                  send({
                    type: "FILL_TECHNICAL",
                    technical: {
                      smartContracts: contracts,
                      managers,
                      programmaticallyGenerated: programmatic,
                      method,
                    },
                    coAuthors: coAuthor.trim() ? [coAuthor.trim()] : [],
                  });
                }}
              >
                <section className="gvsubmitlinkedwearables__section">
                  <div className="gvsubmitlinkedwearables__label">{f.smart_contract.label}</div>
                  <p className="gvsubmitlinkedwearables__sublabel">{f.smart_contract.detail}</p>
                  <ListField
                    values={contracts}
                    placeholder={f.smart_contract.placeholder ?? "Add Ethereum address"}
                    addLabel={f.smart_contract.add_label ?? "Add another address"}
                    onAdd={addTo(setContracts)}
                    onEdit={editAt(setContracts)}
                    onRemove={removeAt(setContracts)}
                  />
                  {errors.smart_contract && (
                    <p className="gvsubmitlinkedwearables__error">{errors.smart_contract}</p>
                  )}
                </section>

                <section className="gvsubmitlinkedwearables__section">
                  <div className="gvsubmitlinkedwearables__label">{f.managers.label}</div>
                  <p className="gvsubmitlinkedwearables__sublabel">{f.managers.detail}</p>
                  <ListField
                    values={managers}
                    placeholder={f.managers.placeholder ?? "Add Ethereum address"}
                    addLabel={f.managers.add_label ?? "Add another address"}
                    onAdd={addTo(setManagers)}
                    onEdit={editAt(setManagers)}
                    onRemove={removeAt(setManagers)}
                  />
                  {errors.managers && (
                    <p className="gvsubmitlinkedwearables__error">{errors.managers}</p>
                  )}
                </section>

                <section className="gvsubmitlinkedwearables__section">
                  <div className="gvsubmitlinkedwearables__label">
                    {f.programmatically_generated.label}
                  </div>
                  <p className="gvsubmitlinkedwearables__sublabel">
                    {f.programmatically_generated.detail}
                  </p>
                  <div className="gvsubmitlinkedwearables__radios">
                    <label className="gvsubmitlinkedwearables__radio">
                      <input
                        type="radio"
                        name="gvlw-programmatic"
                        checked={programmatic}
                        onChange={() => setProgrammatic(true)}
                      />
                      <span className="gvsubmitlinkedwearables__radiomark" aria-hidden="true" />
                      {f.programmatically_generated.yes_label}
                    </label>
                    <label className="gvsubmitlinkedwearables__radio">
                      <input
                        type="radio"
                        name="gvlw-programmatic"
                        checked={!programmatic}
                        onChange={() => setProgrammatic(false)}
                      />
                      <span className="gvsubmitlinkedwearables__radiomark" aria-hidden="true" />
                      {f.programmatically_generated.no_label}
                    </label>
                  </div>
                  <p className="gvsubmitlinkedwearables__postlabel">
                    {f.programmatically_generated.note}
                  </p>
                </section>

                {programmatic && (
                  <section className="gvsubmitlinkedwearables__section">
                    <div className="gvsubmitlinkedwearables__label">
                      {f.method.label}
                      <span className="gvsubmitlinkedwearables__mdnotice">Markdown supported</span>
                    </div>
                    <p className="gvsubmitlinkedwearables__sublabel">{f.method.detail}</p>
                    <textarea
                      className="gvsubmitlinkedwearables__textarea"
                      placeholder={f.method.placeholder ?? ""}
                      value={method}
                      onChange={(e) => setMethod(e.target.value)}
                    />
                    <div className="gvsubmitlinkedwearables__counter">
                      {method.length}/{f.method.max_length ?? 750}
                    </div>
                    {errors.method && (
                      <p className="gvsubmitlinkedwearables__error">{errors.method}</p>
                    )}
                  </section>
                )}

                <section className="gvsubmitlinkedwearables__section">
                  <div className="gvsubmitlinkedwearables__label">{f.coAuthors.label}</div>
                  <p className="gvsubmitlinkedwearables__sublabel">{f.coAuthors.detail}</p>
                  <input
                    className="gvsubmitlinkedwearables__input"
                    placeholder={f.coAuthors.placeholder ?? "Add Ethereum address"}
                    value={coAuthor}
                    onChange={(e) => setCoAuthor(e.target.value)}
                  />
                </section>

                <section className="gvsubmitlinkedwearables__section gvsubmitlinkedwearables__row">
                  <button
                    type="button"
                    className="gvsubmitlinkedwearables__add"
                    onClick={() => send({ type: "BACK" })}
                  >
                    Back
                  </button>
                  <button type="submit" className="gvsubmitlinkedwearables__submit">
                    Review proposal
                  </button>
                </section>
              </form>
            )}

            {value === "review" && (
              <div className="gvsubmitlinkedwearables__form">
                <ReviewRow label={f.name.label} value={ctx.identity.name} />
                <ReviewRow label={f.marketplace_link.label} value={ctx.identity.marketplaceLink} />
                <ReviewRow label={f.links.label} value={ctx.identity.links.filter(Boolean).join(", ")} />
                <ReviewRow
                  label={f.image_previews.label}
                  value={`${ctx.collection.imagePreviews.filter(Boolean).length} image(s)`}
                />
                <ReviewRow label={f.nft_collections.label} value={ctx.collection.nftCollections} />
                <ReviewRow label={f.items.label} value={ctx.collection.items} />
                <ReviewRow
                  label={f.smart_contract.label}
                  value={ctx.technical.smartContracts.filter(Boolean).join(", ")}
                />
                <ReviewRow label={f.managers.label} value={ctx.technical.managers.filter(Boolean).join(", ")} />
                <ReviewRow
                  label={f.programmatically_generated.label}
                  value={ctx.technical.programmaticallyGenerated ? "Yes" : "No"}
                />
                {ctx.technical.programmaticallyGenerated && (
                  <ReviewRow label={f.method.label} value={ctx.technical.method} />
                )}
                {ctx.coAuthors.filter(Boolean).length > 0 && (
                  <ReviewRow label={f.coAuthors.label} value={ctx.coAuthors.filter(Boolean).join(", ")} />
                )}

                <section className="gvsubmitlinkedwearables__section gvsubmitlinkedwearables__row">
                  <button
                    type="button"
                    className="gvsubmitlinkedwearables__add"
                    onClick={() => send({ type: "BACK" })}
                  >
                    Back
                  </button>
                  <button
                    type="button"
                    className="gvsubmitlinkedwearables__submit"
                    onClick={() => send({ type: "SUBMIT" })}
                  >
                    {data.submit_label}
                  </button>
                </section>
              </div>
            )}

            {value === "submitting" && (
              <section className="gvsubmitlinkedwearables__section" role="status">
                <p className="gvsubmitlinkedwearables__desc">
                  Submitting your Linked Wearables Registry proposal to Snapshot&#x2026;
                </p>
              </section>
            )}

            {value === "success" && (
              <section className="gvsubmitlinkedwearables__section" role="status">
                <h2 className="gvsubmitlinkedwearables__h1">{data.success.title}</h2>
                <p className="gvsubmitlinkedwearables__desc">{data.success.lead}</p>
                <p className="gvsubmitlinkedwearables__desc">
                  Proposal id {state.context.result?.id} &#x2014; {data.success.note}
                </p>
              </section>
            )}

            {value === "error" && (
              <section className="gvsubmitlinkedwearables__section" role="alert">
                <p className="gvsubmitlinkedwearables__error">{data.submit_error}</p>
                <p className="gvsubmitlinkedwearables__desc">{state.context.error}</p>
                <div className="gvsubmitlinkedwearables__row">
                  <button
                    type="button"
                    className="gvsubmitlinkedwearables__add"
                    onClick={() => send({ type: "BACK" })}
                  >
                    Back
                  </button>
                  <button
                    type="button"
                    className="gvsubmitlinkedwearables__submit"
                    onClick={() => send({ type: "RETRY" })}
                  >
                    Retry
                  </button>
                </div>
              </section>
            )}
          </div>
        </div>
      </div>
    </GovernanceChrome>
  );
}

function ReviewRow({ label, value }: { label: string; value: string }) {
  return (
    <section className="gvsubmitlinkedwearables__section">
      <div className="gvsubmitlinkedwearables__label">{label}</div>
      <p className="gvsubmitlinkedwearables__desc" style={{ whiteSpace: "pre-wrap" }}>
        {value || "\u{2014}"}
      </p>
    </section>
  );
}
