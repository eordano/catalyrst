import { creatorInfo, openExternal, prettyCat, rarityLabel, type Wearable } from "./Backpack.types";

type BackpackDetailPanelProps = {
  kind: "wearables" | "emotes";
  selected: Wearable | null;
  onClose: () => void;
  equippedSet: Set<string>;
  curBodyShape: string;
  onSetBodyShape: (urn: string) => void;
  onToggleEquip: (w: Wearable) => void;
  activeSlot: number;
  selectedSlot: number | null;
  onAssignEmote: (slot: number, emote: Wearable) => void;
};

export default function BackpackDetailPanel({
  kind,
  selected,
  onClose,
  equippedSet,
  curBodyShape,
  onSetBodyShape,
  onToggleEquip,
  activeSlot,
  selectedSlot,
  onAssignEmote,
}: BackpackDetailPanelProps) {
  const selectedCreator = selected ? creatorInfo(selected) : null;

  return (
    <aside
      className={"bp__info" + (selected ? " is-open" : "")}
      aria-hidden={!selected}
    >
      {selected ? (
        <div className="bp__infosel">
          <button
            type="button"
            className="bp__infoclose"
            aria-label="Deselect item"
            title="Deselect"
            onClick={onClose}
          >
            &#xD7;
          </button>
          <div className="bp__infothumb">
            {selected.thumbnail ? (
              <img src={selected.thumbnail} alt={selected.name ?? ""} />
            ) : null}
          </div>
          <div className="bp__infoname">{selected.name}</div>
          <div className="bp__infometa">
            {kind === "emotes"
              ? `Emote \u{B7} ${prettyCat(selected.category) || "\u{2014}"}`
              : prettyCat(selected.category) || "\u{2014}"}
            {selected.rarity ? ` \u{B7} ${rarityLabel(selected.rarity)}` : ""}
          </div>
          {kind === "emotes" && selected.loop ? (
            <div className="bp__emotetag">&#x1F501; Loops</div>
          ) : null}
          {selected.description ? (
            <p className="bp__infodesc">{selected.description}</p>
          ) : null}
          {selectedCreator ? (
            <button
              type="button"
              className="bp__creatorlink"
              onClick={() => openExternal(selectedCreator.url)}
            >
              <span className="bp__creatorlabel">Creator</span>
              <span className="bp__creatorval">{selectedCreator.label} &#x2197;</span>
            </button>
          ) : null}
          {kind === "wearables" && selected.category === "body_shape" ? (
            (() => {
              const isCurrent =
                (curBodyShape || "").toLowerCase() ===
                (selected.urn || "").toLowerCase();
              return (
                <button
                  type="button"
                  disabled={isCurrent}
                  className={"bp__actionbtn" + (isCurrent ? " is-on" : "")}
                  onClick={() => onSetBodyShape(selected.urn)}
                >
                  {isCurrent ? "Current Body" : "Use Body Shape"}
                </button>
              );
            })()
          ) : kind === "wearables" ? (
            <button
              type="button"
              className={
                "bp__actionbtn" + (equippedSet.has(selected.urn) ? " is-on" : "")
              }
              onClick={() => onToggleEquip(selected)}
            >
              {equippedSet.has(selected.urn) ? "Unequip" : "Equip"}
            </button>
          ) : selectedSlot === activeSlot ? (
            <button type="button" className="bp__assignbtn is-current" disabled>
              Currently in Slot {activeSlot}
            </button>
          ) : (
            <>
              <button
                type="button"
                className="bp__assignbtn"
                onClick={() => onAssignEmote(activeSlot, selected)}
              >
                Assign to Slot {activeSlot}
              </button>
              {selectedSlot != null ? (
                <div className="bp__assigned">Currently in Slot {selectedSlot}</div>
              ) : null}
            </>
          )}
        </div>
      ) : null}
    </aside>
  );
}
