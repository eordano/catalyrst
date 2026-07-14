import ExploreChrome from "../../explorer/frames/ExploreChrome";
import SearchField from "../../atoms/SearchField";
import { AvatarStage, SKIN, HAIRC } from "../../explorer/components/AvatarPreview";
import Spinner from "../../atoms/Spinner";
import "../../explorer/pages/backpack.css";
import "./backpackequipview.css";

const EYE_COLORS = ["#3a6ea5", "#4f8a3d", "#7a4a8c", "#8d5a3c", "#2a2a2a", "#9b2d2d"];

/**
 * `rarity`, `thumbnail` and `description` are null when the item definition did
 * not carry one -- base avatars are prohibited from declaring a rarity at all.
 * Null draws no rarity chip rather than an invented tier.
 */
type BqWearable = {
  urn: string;
  name: string;
  thumbnail: string | null;
  rarity: string | null;
  category: string;
  description: string | null;
};

type BqCategory = { id: string; label: string };

type BqColorKind = "skin" | "hair" | "eye";

type BqSaveResult = { entityId: string; deployed?: boolean };

/**
 * Only "loaded" claims anything about how many items the player owns. A failed
 * ownership read arrives as "unavailable" and must never draw the empty-inventory
 * banner: "you own nothing" and "we could not ask" are different sentences.
 */
type BqInventory =
  | { status: "loaded"; empty: boolean }
  | { status: "not-connected" }
  | { status: "unavailable"; reason: string };

type BqCatalogState =
  | { status: "complete" }
  | { status: "partial"; reason: string }
  | { status: "unavailable"; reason: string };

type BackpackEquipViewProps<W extends BqWearable> = {
  value?: string;
  step?: string;
  catalog?: W[];
  categories?: BqCategory[];
  activeCat?: string;
  selectedUrn?: string;
  bodyShape?: string;
  wearables?: string[];
  baseWearables?: string[];
  colors?: Record<BqColorKind, string>;
  inventory?: BqInventory;
  catalogState?: BqCatalogState;
  error?: string;
  result?: BqSaveResult;
  onTab?: (id: string) => void;
  onClose?: () => void;
  onSearch?: (value: string) => void;
  onMarketplace?: () => void;
  onPickCategory?: (id: string) => void;
  onInventoryEmpty?: () => void;
  onSelect?: (w: W) => void;
  onEquip?: (w: W) => void;
  onAdjustColors?: () => void;
  onPickColor?: (kind: BqColorKind, color: string) => void;
  onReview?: () => void;
  onBack?: () => void;
  onSave?: () => void;
  onRetry?: () => void;
};

export default function BackpackEquipView<W extends BqWearable>({
  value = "opening",
  step = "open",
  catalog = [],
  categories = [],
  activeCat = "all",
  selectedUrn = undefined,
  bodyShape = undefined,
  wearables = [],
  baseWearables = [],
  colors = { skin: "", hair: "", eye: "" },
  inventory = { status: "loaded", empty: false },
  catalogState = { status: "complete" },
  error = undefined,
  result = undefined,
  onTab = undefined,
  onClose = undefined,
  onSearch = undefined,
  onMarketplace = undefined,
  onPickCategory = undefined,
  onInventoryEmpty = undefined,
  onSelect = undefined,
  onEquip = undefined,
  onAdjustColors = undefined,
  onPickColor = undefined,
  onReview = undefined,
  onBack = undefined,
  onSave = undefined,
  onRetry = undefined,
}: BackpackEquipViewProps<W>) {
  const gridItems =
    activeCat === "all" ? catalog : catalog.filter((w) => w.category === activeCat);

  const selected = selectedUrn
    ? catalog.find((w) => w.urn === selectedUrn)
    : undefined;

  const added = wearables.filter((u) => !baseWearables.includes(u));
  const removed = baseWearables.filter((u) => !wearables.includes(u));

  return (
    <ExploreChrome active="backpack" onTab={onTab} onClose={onClose}>
      <div className="bp bpq" data-step={step}>
        <div className="bp__sub">
          <h1 className="bp__title">Backpack</h1>
          <div className="bp__kinds" role="tablist" aria-label="Backpack section">
            <button role="tab" aria-selected className="bp__kind is-active" type="button">
              <span className="bp__kindicon" aria-hidden>&#x25C7;</span> Wearables
            </button>
          </div>
          <div className="bp__subright">
            <div className="bp__search">
              <SearchField placeholder="Search item" value="" onChange={onSearch} />
            </div>
            <button
              className="bp__marketplace"
              type="button"
              onClick={onMarketplace}
            >
              <span className="bp__mkticon" aria-hidden>&#x1F6CD;</span> Marketplace
            </button>
          </div>
        </div>

        <div className="bp__panes">
          <div className="bp__preview">
            <div className="bp__stage">
              <AvatarStage
                outfit={{
                  bodyShape,
                  wearables,
                  skin: stageColor(colors.skin),
                  hair: stageColor(colors.hair),
                  eyes: stageColor(colors.eye),
                }}
              />
            </div>
            <div className="bpq__equippedcount" aria-live="polite">
              {wearables.length} equipped
            </div>
          </div>

          <section className="bp__center">
            <div className="bp__browse">
              <nav className="bp__rail" aria-label="Categories">
                {categories.map((c) => (
                  <button
                    key={c.id}
                    type="button"
                    className={"bp__cat" + (activeCat === c.id ? " is-active" : "")}
                    title={c.label}
                    aria-label={c.label}
                    onClick={() => onPickCategory?.(c.id)}
                  >
                    <span className="bpq__catlabel">{c.label}</span>
                    <span className="bp__catslot" aria-hidden />
                  </button>
                ))}
              </nav>

              <div className="bp__gridcol">
                {inventory.status === "loaded" && inventory.empty && (
                  <div className="bpq__emptybanner" role="status">
                    Your inventory is empty &#x2014; browsing the marketplace catalog.{" "}
                    <button
                      type="button"
                      className="bpq__link"
                      onClick={onInventoryEmpty}
                    >
                      Why?
                    </button>
                  </div>
                )}
                {inventory.status === "not-connected" && (
                  <div className="bpq__emptybanner" role="status">
                    No wallet is connected, so your items were never looked up &#x2014;
                    browsing the marketplace catalog.
                  </div>
                )}
                {inventory.status === "unavailable" && (
                  <div className="bpq__failbanner" role="alert">
                    We couldn't load your items: {inventory.reason}. This is not
                    an empty inventory {"\u{2014}"} nothing was read. The marketplace
                    catalog is shown meanwhile.
                  </div>
                )}
                {catalogState.status !== "complete" && (
                  <div className="bpq__failbanner" role="alert">
                    {catalogState.status === "unavailable"
                      ? `The wearables catalog couldn't be loaded: ${catalogState.reason}.`
                      : `Some items are missing from this list: ${catalogState.reason}.`}
                  </div>
                )}
                <div className="bp__items bpq__items" role="list">
                  {gridItems.map((w) => (
                    <button
                      key={w.urn}
                      type="button"
                      role="listitem"
                      className={
                        "bpq__tile" +
                        (w.rarity ? " bpq__tile--" + w.rarity : "") +
                        (selectedUrn === w.urn ? " is-selected" : "") +
                        (wearables.includes(w.urn) ? " is-equipped" : "")
                      }
                      title={w.rarity ? `${w.name} \u{B7} ${rarityLabel(w.rarity)}` : w.name}
                      onClick={() => onSelect?.(w)}
                    >
                      {w.thumbnail ? (
                        <img className="bpq__thumb" src={w.thumbnail} alt="" loading="lazy" />
                      ) : (
                        <span className="bpq__thumb bpq__thumb--ph" aria-hidden />
                      )}
                    </button>
                  ))}
                </div>
              </div>
            </div>
          </section>

          <aside className="bp__info bpq__info">
            {(value === "opening" || value === "browsing") && !selected && (
              <div className="bp__infoempty">No item selected</div>
            )}

            {(value === "selecting" || value === "equipping") && selected && (
              <div className="bpq__detail">
                {selected.thumbnail ? (
                  <img className="bpq__detailimg" src={selected.thumbnail} alt="" />
                ) : (
                  <div className="bpq__detailimg bpq__thumb--ph" aria-hidden />
                )}
                {selected.rarity && (
                  <div className={"bpq__rarity bpq__rarity--" + selected.rarity}>
                    {rarityLabel(selected.rarity)}
                  </div>
                )}
                <h2 className="bpq__name">{selected.name}</h2>
                <p className="bpq__cat">Slot: {selected.category.replace(/_/g, " ")}</p>
                {selected.description && (
                  <p className="bpq__desc">{selected.description}</p>
                )}

                <div className="bpq__actions">
                  {value === "selecting" ? (
                    <button
                      type="button"
                      className="bpq__btn bpq__btn--primary"
                      onClick={() => onEquip?.(selected)}
                    >
                      Equip
                    </button>
                  ) : (
                    <>
                      <div className="bpq__equipped-note" role="status">
                        Equipped into {selected.category.replace(/_/g, " ")} {"\u{2014}"} previous
                        occupant unequipped.
                      </div>
                      <button
                        type="button"
                        className="bpq__btn"
                        onClick={onAdjustColors}
                      >
                        Adjust colors
                      </button>
                      <button
                        type="button"
                        className="bpq__btn bpq__btn--primary"
                        onClick={onReview}
                      >
                        Review
                      </button>
                    </>
                  )}
                  <button type="button" className="bpq__btn" onClick={onBack}>
                    Back
                  </button>
                </div>
              </div>
            )}

            {value === "coloring" && (
              <div className="bpq__colors">
                <h2 className="bpq__name">Colors</h2>
                <Swatches kind="skin" label="Skin" colors={SKIN} active={colors.skin} onPick={onPickColor} />
                <Swatches kind="hair" label="Hair" colors={HAIRC} active={colors.hair} onPick={onPickColor} />
                <Swatches kind="eye" label="Eyes" colors={EYE_COLORS} active={colors.eye} onPick={onPickColor} />
                <div className="bpq__actions">
                  <button type="button" className="bpq__btn" onClick={onBack}>
                    Back
                  </button>
                  <button
                    type="button"
                    className="bpq__btn bpq__btn--primary"
                    onClick={onReview}
                  >
                    Review
                  </button>
                </div>
              </div>
            )}

            {value === "reviewing" && (
              <div className="bpq__review">
                <h2 className="bpq__name">Review your avatar</h2>
                <ul className="bpq__diff">
                  {added.map((u) => (
                    <li key={u} className="bpq__diff--add">+ {shortUrn(u, catalog)}</li>
                  ))}
                  {removed.map((u) => (
                    <li key={u} className="bpq__diff--rem">{"\u{2212}"} {shortUrn(u, catalog)}</li>
                  ))}
                  {added.length === 0 && removed.length === 0 && (
                    <li className="bpq__diff--none">No changes vs your current profile.</li>
                  )}
                </ul>
                <div className="bpq__colorrow" aria-label="Colors">
                  <span className="bpq__sw" style={{ background: colors.skin }} title="Skin" />
                  <span className="bpq__sw" style={{ background: colors.hair }} title="Hair" />
                  <span className="bpq__sw" style={{ background: colors.eye }} title="Eyes" />
                </div>
                <p className="bpq__stub">Saving deploys a signed profile entity to /content (SIMULATED).</p>
                <div className="bpq__actions">
                  <button type="button" className="bpq__btn" onClick={onBack}>
                    Back
                  </button>
                  <button
                    type="button"
                    className="bpq__btn bpq__btn--primary"
                    onClick={onSave}
                  >
                    Save avatar
                  </button>
                </div>
              </div>
            )}

            {value === "saving" && (
              <div className="bpq__saving" role="status">
                <Spinner size={36} color="#ff8743" aria-hidden />
                <p>{"Saving your avatar\u{2026} (simulated profile deploy)"}</p>
              </div>
            )}

            {value === "error" && (
              <div className="bpq__saving bpq__saving--error" role="alert">
                <p>Couldn't save: {error}</p>
                {onRetry && (
                  <button
                    type="button"
                    className="bpq__btn bpq__btn--primary"
                    onClick={onRetry}
                  >
                    Retry
                  </button>
                )}
              </div>
            )}

            {value === "done" && (
              <div className="bpq__done" role="status">
                <div className="bpq__check" aria-hidden>{"\u{2713}"}</div>
                <h2 className="bpq__name">Avatar saved</h2>
                <p className="bpq__stub">
                  {result?.deployed ? "Deployed" : "Profile"} entity{" "}
                  <code>{result?.entityId}</code>
                  {result?.deployed
                    ? " \u{2014} signed and persisted to the content server."
                    : " \u{2014} content-address computed; sign in with a wallet to persist it."}
                </p>
              </div>
            )}
          </aside>
        </div>
      </div>
    </ExploreChrome>
  );
}

function Swatches({
  kind,
  label,
  colors,
  active,
  onPick,
}: {
  kind: BqColorKind;
  label: string;
  colors: string[];
  active: string;
  onPick?: (kind: BqColorKind, color: string) => void;
}) {
  return (
    <div className="bpq__swatchrow">
      <span className="bpq__swatchlabel">{label}</span>
      <div className="bpq__swatches" role="group" aria-label={label}>
        {colors.map((c) => (
          <button
            key={c}
            type="button"
            className={"bpq__sw" + (active.toLowerCase() === c.toLowerCase() ? " is-active" : "")}
            style={{ background: c }}
            aria-label={`${label} ${c}`}
            aria-pressed={active.toLowerCase() === c.toLowerCase()}
            onClick={() => onPick?.(kind, c)}
          />
        ))}
      </div>
    </div>
  );
}

function stageColor(hex: string) {
  const m = /^#([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m || m[1] === undefined) return undefined;
  const n = Number.parseInt(m[1], 16);
  return {
    color: {
      r: ((n >> 16) & 255) / 255,
      g: ((n >> 8) & 255) / 255,
      b: (n & 255) / 255,
    },
  };
}

function rarityLabel(rarity: string): string {
  return rarity.charAt(0).toUpperCase() + rarity.slice(1);
}

function shortUrn(urn: string, catalog: BqWearable[]): string {
  const w = catalog.find((x) => x.urn === urn);
  if (w) return w.name;
  return urn.split(":").pop() ?? urn;
}
