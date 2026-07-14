import { useState } from "react";

import ChItemEditor from "../pages/ChItemEditor";
import { AvatarStage } from "../../explorer/components/AvatarPreview";
import MobileEditorGate from "../../components/MobileEditorGate";
import Button from "../../atoms/Button";
import "../pages/chitemeditor.css";
import "./wearableitemeditorview.css";

const STEP_ORDER = ["model", "category", "rarity", "price"] as const;

function chipLabel(value: string): string {
  const spaced = value.replace(/_/g, " ");
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

type CwieItem = { id: string; name: string; type: "wearable" | "emote" };
type CwieDraft = { name?: string; rarity?: string; price?: string; free?: boolean };
type CwieCollection = {
  id: string;
  name: string;
  itemCount?: number;
  status?: string;
};

type WearableItemEditorViewProps = {
  value?: string;
  step?: string;
  draft?: CwieDraft;
  collectionId?: string;
  collectionName?: string;
  collectionStatus?: string;
  itemCount?: number;
  items?: CwieItem[];
  itemsLoaded?: boolean;
  collections?: CwieCollection[];
  categories?: readonly string[];
  rarities?: readonly string[];
  category?: string;
  rarity?: string;
  price?: string;
  free?: boolean;
  maxSupply?: number;
  error?: string;
  onSelectNew?: () => void;
  onSelectItem?: (item: CwieItem) => void;
  onBack?: () => void;
  onNameChange?: (name: string) => void;
  onSetModel?: (fileName?: string) => void;
  onCategoryChange?: (category: string) => void;
  onContinueCategory?: () => void;
  onRarityChange?: (rarity: string) => void;
  onContinueRarity?: () => void;
  onPriceChange?: (price: string) => void;
  onFreeChange?: (free: boolean) => void;
  onSave?: () => void;
  onRetry?: () => void;
  onRevert?: () => void;
  onAddAnother?: () => void;
  onDone?: () => void;
};

export default function WearableItemEditorView({
  value = "selecting",
  step = "select",
  draft = {},
  collectionId = "",
  collectionName = "Collection",
  collectionStatus = "",
  itemCount = 0,
  items = [],
  itemsLoaded = false,
  collections = [],
  categories = [],
  rarities = [],
  category = "",
  rarity = "",
  price = "",
  free = false,
  maxSupply = 0,
  error = "",
  onSelectNew = undefined,
  onSelectItem = undefined,
  onBack = undefined,
  onNameChange = undefined,
  onSetModel = undefined,
  onCategoryChange = undefined,
  onContinueCategory = undefined,
  onRarityChange = undefined,
  onContinueRarity = undefined,
  onPriceChange = undefined,
  onFreeChange = undefined,
  onSave = undefined,
  onRetry = undefined,
  onRevert = undefined,
  onAddAnother = undefined,
  onDone = undefined,
}: WearableItemEditorViewProps) {
  const activeIdx = STEP_ORDER.indexOf(step as (typeof STEP_ORDER)[number]);
  const canRevert = value !== "selecting" && value !== "saving" && value !== "saved";
  const itemName = (draft.name ?? "").trim();
  const [modelFile, setModelFile] = useState<File | null>(null);

  return (
    <div className="cwie-wizard" data-step={step}>
      <MobileEditorGate
        title="Open the wearable editor on a desktop"
        message="The wearable item editor needs a wider screen and a WebGPU-capable desktop browser for the live avatar preview. Come back on a laptop or desktop to keep creating."
        backHref="/create/collections"
        backLabel="Back to your collections"
      />
      <ChItemEditor
        onClose={onDone}
        collection={{
          id: collectionId,
          name: collectionName,
          status: collectionStatus || undefined,
        }}
        collections={collections}
        items={itemsLoaded || items.length > 0 ? items : undefined}
      />

      <div className="cwie-wizard__overlay" role="group" aria-label="Wearable item editor step">
        <div className="cwie-wizard__preview" aria-label="Live avatar preview">
          <AvatarStage className="cwie-wizard__avatar" capabilityGate />
        </div>

        <div className="cwie-wizard__panel">
          {activeIdx >= 0 && (
            <ol className="cwie-wizard__steps" aria-label="Wizard progress">
              {STEP_ORDER.map((s, i) => (
                <li
                  key={s}
                  className={
                    "cwie-wizard__step" +
                    (i === activeIdx ? " cwie-wizard__step--active" : "") +
                    (i < activeIdx ? " cwie-wizard__step--done" : "")
                  }
                >
                  {s}
                </li>
              ))}
            </ol>
          )}

          {value === "selecting" && (
            <>
              <h2 className="cwie-wizard__title">Add or edit a wearable</h2>
              <p className="cwie-wizard__hint">
                {collectionId
                  ? `${collectionName} · ${itemCount} items`
                  : "No collection loaded — this will be drafted as a standalone wearable"}
              </p>
              <div className="cwie-wizard__items">
                <button
                  type="button"
                  className="cwie-wizard__chip cwie-wizard__chip--new"
                  onClick={onSelectNew}
                >
                  + Add a new wearable
                </button>
                {items.map((it) => (
                  <button
                    key={it.id}
                    type="button"
                    className="cwie-wizard__chip"
                    onClick={() => onSelectItem?.(it)}
                  >
                    {it.name} ({it.type})
                  </button>
                ))}
              </div>
              {!itemsLoaded && items.length === 0 && itemCount > 0 && (
                <p className="cwie-wizard__hint">
                  This collection’s existing items can’t be listed right now
                  (the builder item list needs a signed-in session) — you can
                  still add a new wearable.
                </p>
              )}
            </>
          )}

          {value === "model" && (
            <>
              <h2 className="cwie-wizard__title">Model</h2>
              <p className="cwie-wizard__hint">
                Name your wearable and optionally pick a local .glb/.gltf
                model — the file stays on this device (nothing is uploaded)
                and the preview mannequin doesn't render it yet.
              </p>
              <label className="cwie-wizard__name">
                <span className="cwie-wizard__name-label">Name</span>
                <input
                  className="cwie-wizard__name-input"
                  type="text"
                  value={draft.name ?? ""}
                  placeholder="Name your wearable"
                  aria-label="Wearable name"
                  onChange={(e) => onNameChange?.(e.target.value)}
                />
              </label>
              <div className="cwie-wizard__model">
                <label
                  className="cwie-wizard__chip cwie-wizard__model-btn"
                  htmlFor="cwie-model-file"
                >
                  {modelFile ? "Replace local model" : "Choose a local .glb"}
                </label>
                <input
                  id="cwie-model-file"
                  className="cwie-wizard__model-input"
                  type="file"
                  accept=".glb,.gltf"
                  onChange={(e) => {
                    setModelFile(e.target.files?.[0] ?? null);
                    e.target.value = "";
                  }}
                />
                <span className="cwie-wizard__model-file">
                  {modelFile
                    ? `${modelFile.name} — kept locally, not uploaded`
                    : "No file chosen — a placeholder model reference is used"}
                </span>
              </div>
              <div className="cwie-wizard__controls">
                <Button variant="secondary" onClick={onBack}>
                  Back
                </Button>
                <Button
                  variant="primary"
                  disabled={itemName === ""}
                  onClick={() => onSetModel?.(modelFile?.name)}
                >
                  Set model
                </Button>
              </div>
            </>
          )}

          {value === "category" && (
            <>
              <h2 className="cwie-wizard__title">Category</h2>
              <p className="cwie-wizard__hint">
                Which avatar slot does {itemName || "this wearable"} occupy?
              </p>
              <div className="cwie-wizard__options" role="radiogroup" aria-label="Category">
                {categories.map((c) => (
                  <button
                    key={c}
                    type="button"
                    role="radio"
                    aria-checked={category === c}
                    className={
                      "cwie-wizard__chip" +
                      (category === c ? " cwie-wizard__chip--selected" : "")
                    }
                    onClick={() => onCategoryChange?.(c)}
                  >
                    {chipLabel(c)}
                  </button>
                ))}
              </div>
              <div className="cwie-wizard__controls">
                <Button variant="secondary" onClick={onBack}>
                  Back
                </Button>
                <Button variant="primary" onClick={onContinueCategory}>
                  Continue
                </Button>
              </div>
            </>
          )}

          {value === "rarity" && (
            <>
              <h2 className="cwie-wizard__title">Rarity</h2>
              <p className="cwie-wizard__hint">
                Rarity caps how many can ever be minted.
              </p>
              <div className="cwie-wizard__options" role="radiogroup" aria-label="Rarity">
                {rarities.map((r) => (
                  <button
                    key={r}
                    type="button"
                    role="radio"
                    aria-checked={rarity === r}
                    className={
                      "cwie-wizard__chip" +
                      (rarity === r ? " cwie-wizard__chip--selected" : "")
                    }
                    onClick={() => onRarityChange?.(r)}
                  >
                    {chipLabel(r)}
                  </button>
                ))}
              </div>
              <span className="cwie-wizard__supply">
                Max supply: {maxSupply.toLocaleString()} mints
              </span>
              <div className="cwie-wizard__controls">
                <Button variant="secondary" onClick={onBack}>
                  Back
                </Button>
                <Button variant="primary" onClick={onContinueRarity}>
                  Continue
                </Button>
              </div>
            </>
          )}

          {value === "price" && (
            <>
              <h2 className="cwie-wizard__title">Price</h2>
              <p className="cwie-wizard__hint">
                Primary-sale price ({rarity} · {maxSupply.toLocaleString()}{" "}
                supply). Listing is simulated.
              </p>
              <div className="cwie-wizard__price">
                <input
                  className="cwie-wizard__price-input"
                  type="number"
                  min="0"
                  inputMode="decimal"
                  aria-label="MANA price"
                  value={price}
                  disabled={free}
                  onChange={(e) => onPriceChange?.(e.target.value)}
                />
                <span className="cwie-wizard__price-unit">MANA</span>
              </div>
              <label className="cwie-wizard__free">
                <input
                  type="checkbox"
                  checked={free}
                  onChange={(e) => onFreeChange?.(e.target.checked)}
                />
                Give away for free
              </label>
              <ul className="cwie-wizard__splits" aria-label="Sale proceeds (simulated)">
                <li>
                  Primary sale — full price to your beneficiary address
                  (simulated default: your connected wallet).
                </li>
                <li>
                  Secondary sales — a creator royalty goes to the beneficiary;
                  the split is read on-chain at publish time, not set in this
                  preview.
                </li>
              </ul>
              <div className="cwie-wizard__controls">
                <Button variant="secondary" onClick={onBack}>
                  Back
                </Button>
                <Button variant="primary" onClick={onSave}>
                  Save wearable
                </Button>
              </div>
            </>
          )}

          {value === "saving" && (
            <>
              <h2 className="cwie-wizard__title">Saving…</h2>
              <p className="cwie-wizard__hint">
                Uploading model, writing item + listing price (simulated).
              </p>
            </>
          )}

          {value === "saved" && (
            <>
              <h2 className="cwie-wizard__title">Saved</h2>
              <p className="cwie-wizard__hint">
                {itemName || "Your wearable"} · {draft.rarity} ·{" "}
                {draft.free ? "free" : `${draft.price || "0"} MANA`} (simulated persist).
              </p>
              <div className="cwie-wizard__controls">
                <Button variant="secondary" onClick={onAddAnother}>
                  Add another item
                </Button>
                <Button variant="primary" onClick={onDone}>
                  Back to collection
                </Button>
              </div>
            </>
          )}

          {value === "error" && (
            <>
              <h2 className="cwie-wizard__title">Save failed</h2>
              <p className="cwie-wizard__hint">{error}</p>
              <div className="cwie-wizard__controls">
                <Button variant="primary" onClick={onRetry}>
                  Retry
                </Button>
              </div>
            </>
          )}

          {canRevert && (
            <Button
              variant="ghost"
              className="cwie-wizard__revert"
              onClick={onRevert}
            >
              Revert
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
