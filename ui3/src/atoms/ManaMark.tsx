type ManaNetwork = "ethereum" | "polygon" | "matic";

type Glyph = {
  viewBox: string;
  d: string;
  rule: "nonzero" | "evenodd";
};

const GLYPHS = {
  ethereum: {
    viewBox: "0 0 13 14",
    d: "M9.864 6.952C9.864 4.968 8.264 3.32 6.28 3.32C4.296 3.32 2.696 4.968 2.696 6.952C2.696 8.92 4.296 10.52 6.28 10.52C8.264 10.52 9.864 8.92 9.864 6.952ZM9.032 6.952C9.032 8.456 7.688 9.688 6.28 9.688C4.728 9.688 3.528 8.488 3.528 6.952C3.528 5.432 4.728 4.152 6.28 4.152C7.704 4.152 9.032 5.432 9.032 6.952ZM12.248 10.424V3.544L6.28 0.12L0.312 3.544V10.424L6.28 13.848L12.248 10.424ZM11.192 9.832L6.296 12.632L1.368 9.832V4.184L6.28 1.336L11.192 4.184V9.832Z",
    rule: "nonzero",
  },
  polygon: {
    viewBox: "0 0 24 24",
    d: "M12 0L24 12L12 24L0 12L12 0ZM12.0002 3.36001L20.6402 12L12.0002 20.64L3.36023 12L12.0002 3.36001ZM12.0009 16.32C14.3868 16.32 16.3209 14.3859 16.3209 12C16.3209 9.61415 14.3868 7.68002 12.0009 7.68002C9.61507 7.68002 7.68094 9.61415 7.68094 12C7.68094 14.3859 9.61507 16.32 12.0009 16.32Z",
    rule: "evenodd",
  },
} satisfies Record<"ethereum" | "polygon", Glyph>;

type ManaMarkProps = {
  size?: number;
  className?: string;
  network?: ManaNetwork;
};

export default function ManaMark({ size = 16, className, network = "ethereum" }: ManaMarkProps) {
  const g = network === "polygon" || network === "matic" ? GLYPHS.polygon : GLYPHS.ethereum;
  return (
    <svg
      className={className}
      viewBox={g.viewBox}
      width={size}
      height={size}
      fill="currentColor"
      aria-hidden="true"
    >
      <path d={g.d} fillRule={g.rule} clipRule={g.rule} />
    </svg>
  );
}
