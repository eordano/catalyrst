import { siteUrl } from "../../data/site";
import type { ReactNode } from "react";
import { useEffect, useMemo, useRef, useState } from "react";
import SearchField from "../../atoms/SearchField";
import { Avatar } from "../../atoms/primitives";
import EmptyState from "../../components/EmptyState";
import { hexToColor3, baseItemUrn } from "../../data/catalyst/backpack";
import BackpackCategoryTile from "./BackpackCategoryTile";
import BackpackDetailPanel from "./BackpackDetailPanel";
import {
  ALWAYS_VISIBLE_CATS,
  SLOTS,
  openExternal,
  type Base,
  type Equipped,
  type LoadoutEntry,
  type Outfit,
  type Wearable,
} from "./Backpack.types";
import "./backpack.css";

type DclBridge = { send?: (action: string, payload: unknown) => void };
function dclBridge(): DclBridge | undefined {
  return window.dclBridge;
}

const EMOTE_SLOTS = [1, 2, 3, 4, 5, 6, 7, 8, 9, 0] as const;

const PAGE_SIZE = 24;

const OUTFIT_SLOTS = [0, 1, 2, 3, 4];

const COLOR_CATS = new Set<string>(["skin", "hair", "eyes"]);
const COLOR_FIELD: Record<string, "skinColor" | "hairColor" | "eyeColor"> = {
  skin: "skinColor",
  hair: "hairColor",
  eyes: "eyeColor",
};

const SKIN_PALETTE = [
  "#FFE4C6", "#FFDDBC", "#F2C2A5", "#DDB18F", "#CC9B77",
  "#9A765B", "#7D5D47", "#704C38", "#532C1C", "#3C2216",
];
const HAIR_PALETTE = [
  "#1C1C1C", "#3C210B", "#5B310F", "#7B4818", "#985F37",
  "#8C2014", "#E98234", "#FFBE28", "#FAD281", "#D4D4D4",
];
const EYE_PALETTE = [
  "#362626", "#5F3932", "#866142", "#BF9E5A", "#878078",
  "#AFC5C7", "#20B3F6", "#397CB0", "#48DC75", "#3B9F50",
];
const PALETTE: Record<string, string[]> = { skin: SKIN_PALETTE, hair: HAIR_PALETTE, eyes: EYE_PALETTE };

const RARITY_RANK: Record<string, number> = {
  unique: 7, mythic: 6, legendary: 5, exotic: 4, epic: 3, rare: 2, uncommon: 1, common: 0, base: 0,
};

// Windows the page numbers around the current page (first, last, current  1) so a
// large catalog doesn't spam the pager with dozens of buttons.
function pageWindow(current: number, total: number): (number | "\u{2026}")[] {
  if (total <= 7) return Array.from({ length: total }, (_, i) => i);
  const set = new Set(
    [0, total - 1, current - 1, current, current + 1].filter((n) => n >= 0 && n < total),
  );
  const nums = [...set].sort((a, b) => a - b);
  const out: (number | "\u{2026}")[] = [];
  let prev = -2;
  for (const n of nums) {
    if (n - prev > 1) out.push("\u{2026}");
    out.push(n);
    prev = n;
  }
  return out;
}

type BackpackProps = {
  avatarPreview?: ReactNode;
  avatarName?: string;
  catalog?: Wearable[];
  equipped?: Equipped | null;
  emoteCatalog?: Wearable[];
  loading?: boolean;
  error?: string | null;
  onEquippedChange?: ((urns: string[]) => void) | null;
  onPlayEmote?: ((urn: string) => void) | null;
  emoteLoadout?: LoadoutEntry[];
  onEmoteLoadoutChange?: ((loadout: LoadoutEntry[]) => void) | null;
  onBaseChange?: ((base: Base) => void) | null;
  outfits?: Outfit[];
  onOutfitsChange?: ((outfits: Outfit[]) => void) | null;
  renderOutfitPreview?: ((o: Outfit) => ReactNode) | null;
};

export default function Backpack({
  avatarPreview = null,
  avatarName = "",
  catalog = [],
  equipped = null,
  emoteCatalog = [],
  loading = false,
  error = null,
  onEquippedChange = null,
  onPlayEmote = null,
  emoteLoadout = [],
  onEmoteLoadoutChange = null,
  onBaseChange = null,
  outfits = [],
  onOutfitsChange = null,
  renderOutfitPreview = null,
}: BackpackProps) {
  const [kind, setKind] = useState<"wearables" | "emotes">("wearables");
  const [sub, setSub] = useState<"categories" | "outfits">("categories");
  const [cat, setCat] = useState("all");
  const [query, setQuery] = useState("");
  const [selectedUrn, setSelectedUrn] = useState<string | null>(null);
  const [sortBy, setSortBy] = useState<"rarity" | "name">("rarity");
  const [sortDir, setSortDir] = useState<"asc" | "desc">("desc");
  const [collectiblesOnly, setCollectiblesOnly] = useState(false);
  const [sortOpen, setSortOpen] = useState(false);
  const filterRef = useRef<HTMLDivElement>(null);
  const [page, setPage] = useState(0);
  const [localEquipped, setLocalEquipped] = useState<string[] | null>(null);
  const [activeSlot, setActiveSlot] = useState<number>(EMOTE_SLOTS[0]);
  const [localLoadout, setLocalLoadout] = useState<LoadoutEntry[] | null>(null);
  const [localOutfits, setLocalOutfits] = useState<Outfit[] | null>(null);
  const [selectedOutfitSlot, setSelectedOutfitSlot] = useState<number | null>(null);
  const [currentOutfitSlot, setCurrentOutfitSlot] = useState<number | null>(null);
  const [localBase, setLocalBase] = useState<Base | null>(null);

  useEffect(() => {
    if (!sortOpen) return;
    function onDoc(e: MouseEvent) {
      if (filterRef.current && e.target instanceof Node && !filterRef.current.contains(e.target))
        setSortOpen(false);
    }
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [sortOpen]);

  const equippedWearables = useMemo<string[]>(
    () => (localEquipped ?? equipped?.wearables ?? []).map(baseItemUrn),
    [localEquipped, equipped],
  );
  const equippedSet = useMemo(
    () => new Set(equippedWearables),
    [equippedWearables],
  );

  const equippedByCat = useMemo(() => {
    const map: Record<string, Wearable> = {};
    for (const urn of equippedWearables) {
      const w = catalog.find((c) => c.urn === urn);
      if (w && w.category && !map[w.category]) map[w.category] = w;
    }
    return map;
  }, [equippedWearables, catalog]);

  // Only the categories actually present in the catalog get a rail tile (plus the
  // always-on body attributes) -- once the catalog is loaded, hide the rest.
  const visibleSlots = useMemo(() => {
    if (loading || catalog.length === 0) return SLOTS;
    const present = new Set(catalog.map((w) => w.category));
    return SLOTS.filter((s) => ALWAYS_VISIBLE_CATS.has(s.id) || present.has(s.id));
  }, [catalog, loading]);
  useEffect(() => {
    if (cat !== "all" && !visibleSlots.some((s) => s.id === cat)) setCat("all");
  }, [visibleSlots, cat]);

  const curBodyShape =
    localBase?.bodyShape ??
    equipped?.bodyShape ??
    "urn:decentraland:off-chain:base-avatars:BaseMale";
  const curName = localBase?.name ?? equipped?.name ?? avatarName ?? "";
  const curColors: { skin: string; hair: string; eyes: string } = {
    skin: localBase?.skinColor ?? equipped?.skinColor ?? "#c98c63",
    hair: localBase?.hairColor ?? equipped?.hairColor ?? "#5c3824",
    eyes: localBase?.eyeColor ?? equipped?.eyeColor ?? "#3a6ea5",
  };

  function commitBase(next: Base) {
    setLocalBase(next);
    onBaseChange?.(next);
    try {
      dclBridge()?.send?.("SetAvatar", {
        base: {
          bodyShapeUrn: next.bodyShape,
          name: next.name,
          skinColor: hexToColor3(next.skinColor),
          hairColor: hexToColor3(next.hairColor),
          eyesColor: hexToColor3(next.eyeColor),
        },
      });
    } catch {
    }
  }

  function setColor(catId: string, hex: string) {
    const field = COLOR_FIELD[catId];
    if (!field) return;
    commitBase({
      bodyShape: curBodyShape,
      name: curName,
      skinColor: curColors.skin,
      hairColor: curColors.hair,
      eyeColor: curColors.eyes,
      [field]: hex,
    });
  }

  function setBodyShape(urn: string) {
    if (!urn) return;
    commitBase({
      bodyShape: urn,
      name: curName,
      skinColor: curColors.skin,
      hairColor: curColors.hair,
      eyeColor: curColors.eyes,
    });
  }

  function playEmote(urn: string) {
    if (!urn) return;
    onPlayEmote?.(urn);
    try {
      dclBridge()?.send?.("PlayEmote", { urn });
    } catch {
    }
  }

  const items = kind === "emotes" ? emoteCatalog : catalog;

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return items.filter((w) => {
      if (kind === "wearables" && cat !== "all" && w.category !== cat)
        return false;
      if (collectiblesOnly && (w.rarity || "base").toLowerCase() === "base")
        return false;
      if (
        q &&
        !(w.name || "").toLowerCase().includes(q) &&
        !(w.urn || "").toLowerCase().includes(q)
      )
        return false;
      return true;
    });
  }, [items, kind, cat, query, collectiblesOnly]);

  const sorted = useMemo(() => {
    const dir = sortDir === "asc" ? 1 : -1;
    const arr = [...filtered];
    arr.sort((a, b) =>
      sortBy === "name"
        ? dir * (a.name || "").localeCompare(b.name || "")
        : dir *
          ((RARITY_RANK[(a.rarity || "").toLowerCase()] ?? 0) -
            (RARITY_RANK[(b.rarity || "").toLowerCase()] ?? 0)),
    );
    return arr;
  }, [filtered, sortBy, sortDir]);

  const pageCount = Math.max(1, Math.ceil(sorted.length / PAGE_SIZE));
  const currentPage = Math.min(page, pageCount - 1);
  const pageItems = useMemo(
    () =>
      sorted.slice(currentPage * PAGE_SIZE, currentPage * PAGE_SIZE + PAGE_SIZE),
    [sorted, currentPage],
  );
  useEffect(() => {
    setPage(0);
  }, [kind, cat, query, sortBy, sortDir, collectiblesOnly]);

  const selected = useMemo(
    () => items.find((w) => w.urn === selectedUrn) ?? null,
    [items, selectedUrn],
  );

  const loadout = localLoadout ?? emoteLoadout ?? [];
  const emoteByUrn = useMemo(() => {
    const m: Record<string, Wearable> = {};
    for (const e of emoteCatalog) m[e.urn] = e;
    return m;
  }, [emoteCatalog]);
  const loadoutBySlot = useMemo(() => {
    const m: Record<number, LoadoutEntry> = {};
    for (const b of loadout) m[b.slot] = b;
    return m;
  }, [loadout]);
  const slotByUrn = useMemo(() => {
    const m: Record<string, number> = {};
    for (const b of loadout) m[b.urn] = b.slot;
    return m;
  }, [loadout]);
  const selectedSlot = useMemo(() => {
    const b = loadout.find((x) => x.urn === selectedUrn);
    return b ? b.slot : null;
  }, [loadout, selectedUrn]);

  function assignEmote(slot: number, emote: Wearable) {
    if (!emote) return;
    const next = loadout
      .filter((b) => b.slot !== slot && b.urn !== emote.urn)
      .concat({ slot, urn: emote.urn, name: emote.name });
    setLocalLoadout(next);
    onEmoteLoadoutChange?.(next);
  }

  const outfitsList = localOutfits ?? outfits ?? [];
  const outfitsBySlot = useMemo(() => {
    const m: Record<number, Outfit> = {};
    for (const o of outfitsList) m[o.slot] = o;
    return m;
  }, [outfitsList]);
  const catalogByUrn = useMemo(() => {
    const m: Record<string, Wearable> = {};
    for (const w of catalog) m[w.urn] = w;
    return m;
  }, [catalog]);

  function saveOutfit() {
    const slot = OUTFIT_SLOTS.find((s) => !outfitsBySlot[s]);
    if (slot === undefined) return;
    const next = [
      ...outfitsList,
      {
        slot,
        bodyShape: curBodyShape,
        wearables: [...equippedWearables],
        skinColor: curColors.skin,
        hairColor: curColors.hair,
        eyeColor: curColors.eyes,
      },
    ];
    setLocalOutfits(next);
    onOutfitsChange?.(next);
  }

  function wearOutfit(o: Outfit) {
    if (!o) return;
    const urns = [...(o.wearables || [])];
    setLocalEquipped(urns);
    onEquippedChange?.(urns);
    commitBase({
      bodyShape: o.bodyShape || curBodyShape,
      name: curName,
      skinColor: o.skinColor || curColors.skin,
      hairColor: o.hairColor || curColors.hair,
      eyeColor: o.eyeColor || curColors.eyes,
    });
    try {
      dclBridge()?.send?.("SetAvatar", {
        equip: {
          wearableUrns: urns,
          emoteUrns: o.emotes ?? equipped?.emotes ?? [],
          forceRender: [],
        },
      });
    } catch {
    }
  }

  function removeOutfit(slot: number) {
    const next = outfitsList.filter((o) => o.slot !== slot);
    setLocalOutfits(next);
    onOutfitsChange?.(next);
    if (selectedOutfitSlot === slot) setSelectedOutfitSlot(null);
    if (currentOutfitSlot === slot) setCurrentOutfitSlot(null);
  }

  function previewOutfit(slot: number, o: Outfit) {
    setSelectedOutfitSlot(slot);
    wearOutfit(o);
  }
  function wearOutfitAsCurrent(slot: number, o: Outfit) {
    previewOutfit(slot, o);
    setCurrentOutfitSlot(slot);
  }

  function toggleEquip(w: Wearable) {
    if (!w) return;
    const set = new Set(equippedWearables);
    if (set.has(w.urn)) {
      set.delete(w.urn);
    } else {
      for (const urn of [...set]) {
        const cw = catalog.find((c) => c.urn === urn);
        if (cw && cw.category === w.category) set.delete(urn);
      }
      set.add(w.urn);
    }
    const next = [...set];
    setLocalEquipped(next);
    onEquippedChange?.(next);
    setSelectedOutfitSlot(null);
    setCurrentOutfitSlot(null);
    try {
      dclBridge()?.send?.("SetAvatar", {
        equip: {
          wearableUrns: next,
          emoteUrns: equipped?.emotes ?? [],
          forceRender: [],
        },
      });
    } catch {
    }
  }

  // Hover/focus preview: shows the swap on the live avatar without persisting it, so
  // browsing the grid never commits -- only an explicit Equip action does.
  function previewSetFor(w: Wearable): string[] {
    if (kind !== "wearables" || w.category === "body_shape" || equippedSet.has(w.urn))
      return equippedWearables;
    return equippedWearables
      .filter((urn) => catalogByUrn[urn]?.category !== w.category)
      .concat(w.urn);
  }
  function beginPreview(w: Wearable) {
    onEquippedChange?.(previewSetFor(w));
  }
  function endPreview() {
    onEquippedChange?.(equippedWearables);
  }

  return (
      <div className="bp">
        <div className="bp__sub">
          <h1 className="bp__title">Backpack</h1>
          <div className="bp__kinds" role="tablist" aria-label="Backpack section">
            <button
              role="tab"
              aria-selected={kind === "wearables"}
              className={"bp__kind" + (kind === "wearables" ? " is-active" : "")}
              onClick={() => {
                setKind("wearables");
                setSelectedUrn(null);
              }}
            >
              <span className="bp__kindicon" aria-hidden>&#x25C7;</span> Wearables
            </button>
            <button
              role="tab"
              aria-selected={kind === "emotes"}
              className={"bp__kind" + (kind === "emotes" ? " is-active" : "")}
              onClick={() => {
                setKind("emotes");
                setSelectedUrn(null);
              }}
            >
              <span className="bp__kindicon" aria-hidden>&#x266A;</span> Emotes
            </button>
          </div>
          <div className="bp__subright">
            <div className="bp__filterwrap" ref={filterRef}>
              <button
                className={"bp__filter" + (sortOpen ? " is-open" : "")}
                type="button"
                aria-haspopup="true"
                aria-expanded={sortOpen}
                onClick={() => setSortOpen((o) => !o)}
              >
                Filter &amp; Sort <span className="bp__caret" aria-hidden>&#x25BE;</span>
              </button>
              {sortOpen && (
                <div className="bp__filterpop" role="group" aria-label="Filter and sort items">
                  <span className="bp__filterlabel">Sort by</span>
                  <div className="bp__filterrow">
                    <button
                      type="button"
                      className={"bp__filteropt" + (sortBy === "rarity" ? " is-active" : "")}
                      onClick={() => setSortBy("rarity")}
                    >
                      Rarity
                    </button>
                    <button
                      type="button"
                      className={"bp__filteropt" + (sortBy === "name" ? " is-active" : "")}
                      onClick={() => setSortBy("name")}
                    >
                      Name
                    </button>
                  </div>
                  <span className="bp__filterlabel">Order</span>
                  <div className="bp__filterrow">
                    <button
                      type="button"
                      className={"bp__filteropt" + (sortDir === "desc" ? " is-active" : "")}
                      onClick={() => setSortDir("desc")}
                    >
                      {sortBy === "name" ? "Z \u{2013} A" : "Rarest"}
                    </button>
                    <button
                      type="button"
                      className={"bp__filteropt" + (sortDir === "asc" ? " is-active" : "")}
                      onClick={() => setSortDir("asc")}
                    >
                      {sortBy === "name" ? "A \u{2013} Z" : "Common"}
                    </button>
                  </div>
                  <label className="bp__filtercheck">
                    <input
                      type="checkbox"
                      checked={collectiblesOnly}
                      onChange={(e) => setCollectiblesOnly(e.target.checked)}
                    />
                    Collectibles only
                  </label>
                </div>
              )}
            </div>
            <div className="bp__search">
              <SearchField
                placeholder="Search item"
                value={query}
                onChange={setQuery}
              />
            </div>
            <button
              className="bp__marketplace"
              type="button"
              onClick={() => openExternal(siteUrl("/shop"))}
            >
              <span className="bp__mkticon" aria-hidden>&#x1F6CD;</span> Marketplace
            </button>
          </div>
        </div>

        <div className="bp__panes">
          <div className="bp__preview">
            <div className="bp__stage">
              {avatarPreview ? (
                avatarPreview
              ) : (
                <Avatar size={180} name="DCL Avatar" className="bp__avatar" />
              )}
            </div>
            <button
              className="bp__help"
              type="button"
              aria-label="Help"
              onClick={() => window.open("https://docs.decentraland.org/player/", "_blank", "noopener,noreferrer")}
            >
              ?
            </button>
          </div>

          <section className="bp__center">
            {kind === "wearables" ? (
              <div className="bp__subtabs" role="tablist" aria-label="Browse">
                <button
                  role="tab"
                  aria-selected={sub === "categories"}
                  className={
                    "bp__subtab" + (sub === "categories" ? " is-active" : "")
                  }
                  onClick={() => setSub("categories")}
                >
                  &#x2630; Categories
                </button>
                <button
                  role="tab"
                  aria-selected={sub === "outfits"}
                  className={
                    "bp__subtab" + (sub === "outfits" ? " is-active" : "")
                  }
                  onClick={() => {
                    setSub("outfits");
                    setSelectedUrn(null);
                  }}
                >
                  &#x2B13; Saved Outfits
                </button>
              </div>
            ) : null}

            <div className="bp__browse">
              {sub === "outfits" && kind === "wearables" ? (
                <div className="bp__outfits">
                  <div className="bp__oslots">
                    <button
                      type="button"
                      className="bp__oslot bp__oslot--save"
                      onClick={saveOutfit}
                      disabled={OUTFIT_SLOTS.every((s) => outfitsBySlot[s])}
                      title="Save the current look as an outfit"
                    >
                      <span className="bp__oplus" aria-hidden>
                        +
                      </span>
                      <span className="bp__oslotlabel">SAVE OUTFIT</span>
                    </button>
                    {OUTFIT_SLOTS.map((s) => {
                      const o = outfitsBySlot[s];
                      if (!o)
                        return (
                          <div
                            key={s}
                            className="bp__oslot bp__oslot--empty"
                          >
                            <svg
                              className="bp__osil"
                              viewBox="0 0 60 130"
                              width="42"
                              height="92"
                              aria-hidden
                            >
                              <circle cx="30" cy="20" r="14" />
                              <path d="M14 56c0-9 7-16 16-16s16 7 16 16v34c0 6-32 6-32 0V56Z" />
                            </svg>
                            <span className="bp__oemptylabel">Empty Slot</span>
                          </div>
                        );
                      const thumbs = (o.wearables || [])
                        .flatMap((u) => {
                          const t = catalogByUrn[u]?.thumbnail;
                          return t ? [t] : [];
                        })
                        .slice(0, 4);
                      const isCurrent = currentOutfitSlot === s;
                      const isSelected =
                        selectedOutfitSlot === s && !isCurrent;
                      return (
                        <div
                          key={s}
                          className={
                            "bp__oslot bp__oslot--filled" +
                            (isCurrent
                              ? " is-current"
                              : isSelected
                                ? " is-selected"
                                : "")
                          }
                        >
                          <div
                            className="bp__opreview"
                            role="button"
                            tabIndex={0}
                            title={`Preview Outfit ${s + 1}`}
                            onClick={() => previewOutfit(s, o)}
                            onKeyDown={(e) => {
                              if (e.key === "Enter" || e.key === " ") {
                                e.preventDefault();
                                previewOutfit(s, o);
                              }
                            }}
                          >
                            {renderOutfitPreview ? (
                              renderOutfitPreview(o)
                            ) : (
                              <span className="bp__ocollage">
                                {thumbs.map((src, i) => (
                                  <img key={i} src={src} alt="" loading="lazy" />
                                ))}
                              </span>
                            )}
                          </div>
                          <span className="bp__oslotlabel">Outfit {s + 1}</span>
                          <div className="bp__oactions">
                            {isCurrent ? (
                              <button
                                type="button"
                                className="bp__owear is-current"
                                disabled
                              >
                                Current
                              </button>
                            ) : (
                              <button
                                type="button"
                                className="bp__owear"
                                onClick={() => wearOutfitAsCurrent(s, o)}
                              >
                                Wear
                              </button>
                            )}
                            {isSelected ? (
                              <button
                                type="button"
                                className="bp__oremovebtn"
                                onClick={() => removeOutfit(s)}
                              >
                                Remove
                              </button>
                            ) : null}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                  <div className="bp__opromo">
                    <div className="bp__opromotitle">
                      Unlock 5 more Outfit slots by getting a NAME!
                    </div>
                    <p className="bp__opromotext">
                      NAMEs are unique Decentraland usernames that come with a
                      badge, the ability to create Communities, 5 more Outfit
                      slots, and 100 Voting Power in the DAO.
                    </p>
                    <button
                      type="button"
                      className="bp__getname"
                      onClick={() =>
                        openExternal(siteUrl("/marketplace/claim-name"))
                      }
                    >
                      GET A NAME
                    </button>
                  </div>
                </div>
              ) : (
                <>
              {kind === "wearables" ? (
                <nav className="bp__rail" aria-label="Categories">
                  {visibleSlots.map((s) => (
                    <BackpackCategoryTile
                      key={s.id}
                      slot={s}
                      active={cat === s.id}
                      thumbnail={equippedByCat[s.id]?.thumbnail}
                      onClick={() => setCat(s.id)}
                    />
                  ))}
                </nav>
              ) : null}

              {kind === "emotes" ? (
                <nav className="bp__emoteslots" aria-label="Emote wheel slots">
                  {EMOTE_SLOTS.map((n) => {
                    const b = loadoutBySlot[n];
                    const e = b ? emoteByUrn[b.urn] : null;
                    const label = e?.name || b?.name || "Empty";
                    return (
                      <button
                        key={n}
                        type="button"
                        className={"bp__eslot" + (activeSlot === n ? " is-active" : "")}
                        onClick={() => {
                          setActiveSlot(n);
                          setSelectedUrn(b?.urn ?? null);
                        }}
                        aria-pressed={activeSlot === n}
                        title={label}
                      >
                        <span className="bp__eslotnum">{n}</span>
                        <span className="bp__eslotname u-truncate">{label}</span>
                        <span className="bp__eslotart" aria-hidden>
                          {e?.thumbnail ? (
                            <img src={e.thumbnail} alt="" loading="lazy" />
                          ) : null}
                        </span>
                      </button>
                    );
                  })}
                </nav>
              ) : null}

              <div className="bp__gridcol">
                {kind === "wearables" && !COLOR_CATS.has(cat) ? (
                  <button
                    className="bp__allpill"
                    onClick={() => setCat("all")}
                    aria-pressed={cat === "all"}
                    type="button"
                  >
                    <span className="bp__allinfinity" aria-hidden>&#x221E;</span> ALL
                  </button>
                ) : null}

                {kind === "wearables" && COLOR_CATS.has(cat) ? (
                  <div
                    className="bp__swatches"
                    role="list"
                    aria-label={`${cat} colors`}
                  >
                    {(PALETTE[cat] ?? []).map((hex) => {
                      const isSel =
                        (curColors[cat as "skin" | "hair" | "eyes"] || "").toLowerCase() ===
                        hex.toLowerCase();
                      return (
                        <button
                          key={hex}
                          type="button"
                          role="listitem"
                          title={hex}
                          aria-label={hex}
                          aria-pressed={isSel}
                          className={"bp__swatch" + (isSel ? " is-on" : "")}
                          style={{ background: hex }}
                          onClick={() => setColor(cat, hex)}
                        />
                      );
                    })}
                  </div>
                ) : null}

                {kind === "wearables" && cat === "skin" ? null : loading &&
                  items.length === 0 ? (
                  <div className="bp__items" role="list">
                    {Array.from({ length: 16 }).map((_, i) => (
                      <span key={i} className="bp__item" role="listitem" />
                    ))}
                  </div>
                ) : filtered.length === 0 ? (
                  <EmptyState
                    variant="inline"
                    title={error ? "Couldn't load items" : "No items found"}
                    subtitle={error || (query ? `No matches for "${query}"` : undefined)}
                  />
                ) : (
                  <>
                    <div className="bp__items" role="list">
                      {pageItems.map((w) => {
                        const assignedSlot =
                          kind === "emotes" ? slotByUrn[w.urn] : undefined;
                        const isAssigned = assignedSlot !== undefined;
                        const isEquipped =
                          equippedSet.has(w.urn) ||
                          (w.category === "body_shape" &&
                            (curBodyShape || "").toLowerCase() ===
                              (w.urn || "").toLowerCase());
                        const canEquip =
                          kind === "wearables" && w.category !== "body_shape";
                        const hoverProps =
                          kind === "wearables"
                            ? {
                                onMouseEnter: () => beginPreview(w),
                                onMouseLeave: endPreview,
                                onFocus: () => beginPreview(w),
                                onBlur: endPreview,
                              }
                            : {};
                        return (
                          <div
                            key={w.urn}
                            className={
                              "bp__item" +
                              (selectedUrn === w.urn ? " is-selected" : "") +
                              (isEquipped ? " is-equipped" : "") +
                              (isAssigned ? " is-assigned" : "")
                            }
                            role="listitem"
                            {...hoverProps}
                          >
                            <button
                              className="bp__itemhit"
                              title={
                                kind === "emotes" ? `Preview ${w.name}` : (w.name ?? undefined)
                              }
                              type="button"
                              onClick={() => {
                                if (kind === "emotes") {
                                  setSelectedUrn((cur) => (cur === w.urn ? null : w.urn));
                                  playEmote(w.urn);
                                } else {
                                  setSelectedUrn(w.urn);
                                  toggleEquip(w);
                                }
                              }}
                            >
                              {w.thumbnail ? (
                                <img src={w.thumbnail} alt={w.name ?? ""} loading="lazy" />
                              ) : null}
                            </button>
                            {isAssigned ? (
                              <span className="bp__slotbadge" aria-hidden>
                                {assignedSlot}
                              </span>
                            ) : null}
                            {canEquip ? (
                              <button
                                type="button"
                                className={
                                  "bp__itemequip" + (isEquipped ? " is-on" : "")
                                }
                                aria-label={`${isEquipped ? "Unequip" : "Equip"} ${w.name ?? ""}`.trim()}
                                onClick={() => {
                                  setSelectedUrn(w.urn);
                                  toggleEquip(w);
                                }}
                              >
                                {isEquipped ? "Unequip" : "Equip"}
                              </button>
                            ) : null}
                          </div>
                        );
                      })}
                    </div>
                    {pageCount > 1 ? (
                      <div className="bp__pager" role="navigation" aria-label="Pagination">
                        <button
                          type="button"
                          className="bp__pagebtn"
                          aria-label="Previous page"
                          disabled={currentPage === 0}
                          onClick={() => setPage((p) => Math.max(0, p - 1))}
                        >
                          &#x2039;
                        </button>
                        {pageWindow(currentPage, pageCount).map((n, i) =>
                          n === "\u{2026}" ? (
                            <span key={"dots" + i} className="bp__pagedots" aria-hidden>
                              &#x2026;
                            </span>
                          ) : (
                            <button
                              key={n}
                              type="button"
                              className={"bp__pagenum" + (n === currentPage ? " is-active" : "")}
                              aria-current={n === currentPage ? "page" : undefined}
                              aria-label={`Page ${n + 1}`}
                              onClick={() => setPage(n)}
                            >
                              {n + 1}
                            </button>
                          ),
                        )}
                        <button
                          type="button"
                          className="bp__pagebtn"
                          aria-label="Next page"
                          disabled={currentPage >= pageCount - 1}
                          onClick={() =>
                            setPage((p) => Math.min(pageCount - 1, p + 1))
                          }
                        >
                          &#x203A;
                        </button>
                      </div>
                    ) : null}
                  </>
                )}
              </div>
                </>
              )}
            </div>
          </section>

          <BackpackDetailPanel
            kind={kind}
            selected={selected}
            onClose={() => setSelectedUrn(null)}
            equippedSet={equippedSet}
            curBodyShape={curBodyShape}
            onSetBodyShape={setBodyShape}
            onToggleEquip={toggleEquip}
            activeSlot={activeSlot}
            selectedSlot={selectedSlot}
            onAssignEmote={assignEmote}
          />
        </div>
      </div>
  );
}
