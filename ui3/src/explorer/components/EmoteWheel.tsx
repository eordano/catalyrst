import type { MouseEvent } from "react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { sendBridge } from "../../overlay/bridge";
import { baseItemUrn } from "../../data/catalyst/backpack";
import type { Emote, SlotBinding } from "../../data/catalyst/backpack";
import EmoteSlot from "./EmoteSlot";
import "./emotewheel.css";

const CENTER = 215;
const RADIUS = 152;
const CARD_W = 112;

type WheelSlot = { num: number; deg: number; emote?: Emote };

function buildSlots(catalog: Emote[], loadout: SlotBinding[]): WheelSlot[] {
  const byUrn = new Map(catalog.map((e) => [baseItemUrn(e.urn), e]));
  const boundUrn = new Map(loadout.map((b) => [b.slot, b.urn]));
  return Array.from({ length: 10 }, (_, k) => {
    const num = (k + 1) % 10;
    const urn = boundUrn.get(num);
    return {
      num,
      deg: -90 + k * 36,
      emote: urn ? byUrn.get(baseItemUrn(urn)) : undefined,
    };
  });
}

type EmoteWheelProps = {
  catalog?: Emote[];
  loadout?: SlotBinding[];
  onSelect?: (name: string) => void;
  onClose?: () => void;
  onCustomise?: () => void;
};

export default function EmoteWheel({
  catalog = [],
  loadout = [],
  onSelect,
  onClose,
  onCustomise,
}: EmoteWheelProps) {
  const slots = useMemo(() => buildSlots(catalog, loadout), [catalog, loadout]);
  const [hover, setHover] = useState<number | null>(null);

  const choose = useCallback(
    (emote: Emote | undefined) => {
      if (!emote) return;
      sendBridge("PlayEmote", { urn: emote.urn });
      onSelect?.(emote.name || emote.urn);
    },
    [onSelect],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if ((e.key === "e" || e.key === "E") && onCustomise) {
        e.preventDefault();
        e.stopPropagation();
        onCustomise();
        return;
      }
      if (!/^[0-9]$/.test(e.key)) return;
      const num = Number(e.key);
      const slot = slots.find((s) => s.num === num);
      if (slot?.emote) {
        setHover(num);
        choose(slot.emote);
      }
      e.preventDefault();
      e.stopPropagation();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [choose, slots, onCustomise]);

  const hovered = hover != null ? slots.find((s) => s.num === hover)?.emote : undefined;

  return (
    <div
      className="ew"
      onClick={(e: MouseEvent<HTMLDivElement>) => {
        if (e.target === e.currentTarget) onClose?.();
      }}
    >
      <div className="ew__wheel">
        <button
          type="button"
          className="ew__close"
          aria-label="Close emotes"
          title="Close"
          onClick={() => onClose?.()}
        >
          &#xD7;
        </button>

        {slots.map(({ num, deg, emote }) => {
          const rad = (deg * Math.PI) / 180;
          const cx = CENTER + RADIUS * Math.cos(rad);
          const cy = CENTER + RADIUS * Math.sin(rad);
          return (
            <div
              key={num}
              role="button"
              aria-label={emote ? emote.name || emote.urn : `Empty emote slot ${num}`}
              className={
                "ew__slotpos" +
                (hover === num ? " ew__slotpos--hover" : "") +
                (emote ? "" : " ew__slotpos--empty")
              }
              style={{ left: `${cx}px`, top: `${cy}px` }}
              tabIndex={emote ? 0 : -1}
              onMouseEnter={() => emote && setHover(num)}
              onMouseLeave={() => setHover((h) => (h === num ? null : h))}
              onFocus={() => emote && setHover(num)}
              onBlur={() => setHover((h) => (h === num ? null : h))}
              onClick={() => choose(emote)}
              onKeyDown={(e) => {
                if (e.key !== "Enter" && e.key !== " ") return;
                e.preventDefault();
                choose(emote);
              }}
            >
              <EmoteSlot
                thumbnail={emote?.thumbnail}
                rarity={emote?.rarity ?? "base"}
                number={num}
                rotate={deg + 90}
                width={CARD_W}
              />
            </div>
          );
        })}

        <div className="ew__center">
          <div className="ew__hovername">{hovered?.name ?? " "}</div>
          <div className="ew__title">EMOTES</div>
          {onCustomise && (
            <button
              type="button"
              className="ew__customise"
              data-sb-linkto="Explorer/Pages/BackpackEmotes"
              onClick={() => onCustomise()}
            >
              Customise [E]
            </button>
          )}
          <div className="ew__hint">
            Hold [B+num] to run an emote
            <br />
            while the wheel is closed
          </div>
        </div>
      </div>
    </div>
  );
}
