type Tone = "unavailable" | "stale";

type Props = {
  tone: Tone;
  title: string;
  detail?: string;
};

export default function GovernanceNotice({ tone, title, detail }: Props) {
  return (
    <div
      role="status"
      data-tone={tone}
      style={{ ...WRAP, ...(tone === "unavailable" ? UNAVAILABLE : STALE) }}
    >
      <strong style={TITLE}>{title}</strong>
      {detail && <span style={DETAIL}>{detail}</span>}
    </div>
  );
}

const WRAP: React.CSSProperties = {
  display: "flex",
  flexDirection: "column",
  gap: 4,
  margin: "16px 0",
  padding: "12px 16px",
  borderRadius: 8,
  borderWidth: 1,
  borderStyle: "solid",
  fontSize: 14,
  lineHeight: 1.5,
};
const UNAVAILABLE: React.CSSProperties = {
  borderColor: "rgba(255,45,85,.45)",
  background: "rgba(255,45,85,.08)",
  color: "#fcfcfc",
};
const STALE: React.CSSProperties = {
  borderColor: "rgba(255,255,255,.22)",
  background: "rgba(255,255,255,.05)",
  color: "rgba(255,255,255,.82)",
};
const TITLE: React.CSSProperties = { fontWeight: 700 };
const DETAIL: React.CSSProperties = { opacity: 0.85 };
