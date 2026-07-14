import type { CSSProperties } from "react";
import { asset } from "../../asset";
import { iconFor, type SlotDef } from "./Backpack.types";

type CategoryTileProps = {
  slot: SlotDef;
  active: boolean;
  thumbnail?: string | null;
  onClick: () => void;
};

export default function BackpackCategoryTile({
  slot,
  active,
  thumbnail,
  onClick,
}: CategoryTileProps) {
  const glyphStyle: CSSProperties & { "--i": string } = {
    "--i": `url(${asset("assets/categories/" + iconFor(slot.id) + ".png")})`,
  };
  return (
    <button
      className={"bp__cat" + (active ? " is-active" : "")}
      onClick={onClick}
      title={slot.label}
      aria-label={slot.label}
    >
      <span className="bp__catglyph u-mask-icon" aria-hidden style={glyphStyle} />
      <span className="bp__catslot" aria-hidden>
        {thumbnail ? (
          <img className="bp__catthumb" src={thumbnail} alt="" loading="lazy" />
        ) : null}
      </span>
    </button>
  );
}
