import ManaMark from "../../atoms/ManaMark";
import DclLogomark from "../../atoms/DclLogomark";
import "./enscard.css";

type EnsGradient = { min: number; max: number; c: [string, string] };

const ENS_GRADIENTS: EnsGradient[] = [
  { min: 2, max: 3, c: ["#C640CD", "#691FA9"] },
  { min: 4, max: 5, c: ["#FF2D55", "#FFBC5B"] },
  { min: 5, max: 6, c: ["#73FFAF", "#1A9850"] },
  { min: 7, max: 8, c: ["#81D1FF", "#3077E1"] },
  { min: 9, max: 10, c: ["#F6C1FF", "#FF4BED"] },
  { min: 11, max: 15, c: ["#FF9EB1", "#FF2D55"] },
];

export function ensGradient(name?: string): string {
  const n = (name || "").length;
  const g = ENS_GRADIENTS.find((x) => n >= x.min && n <= x.max) || { c: ["#000", "#fff"] as [string, string] };
  return `linear-gradient(135deg, ${g.c[0]} 0%, ${g.c[1]} 100%)`;
}

type EnsCardProps = {
  name: string;
  price?: string | number | null;
  onClick?: () => void;
};

export default function EnsCard({ name, price, onClick }: EnsCardProps) {
  return (
    <button type="button" className="enscard" onClick={onClick}>
      <span className="enscard__art" aria-hidden="true" style={{ background: ensGradient(name) }}>
        <span className="enscard__mark">
          <DclLogomark size={40} />
        </span>
        <span className="enscard__name">{name}</span>
        <span className="enscard__sub">DCL.ETH</span>
      </span>

      <span className="enscard__body">
        <span className="enscard__row">
          <span className="enscard__title u-truncate">{name}</span>
          {price != null ? (
            <span className="enscard__price">
              <ManaMark size={13} className="enscard__manamark" />
              {price}
            </span>
          ) : null}
        </span>
        <span className="enscard__meta">
          <span className="enscard__network">Ethereum</span>
          <span className="enscard__badge">Name</span>
        </span>
      </span>
    </button>
  );
}
