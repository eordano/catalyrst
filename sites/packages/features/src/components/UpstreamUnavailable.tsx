type Props = {
  title?: string;
  message?: string;
  backHref?: string;
  backLabel?: string;
};

export default function UpstreamUnavailable({
  title = "Temporarily unavailable",
  message = "We couldn't load this page right now. Please try again in a moment.",
  backHref,
  backLabel,
}: Props) {
  return (
    <div style={WRAP}>
      <h1 style={TITLE}>{title}</h1>
      <p style={SUB}>{message}</p>
      {backHref && (
        <a href={backHref} style={LINK}>
          &#x2190; {backLabel ?? "Go back"}
        </a>
      )}
    </div>
  );
}

const WRAP: React.CSSProperties = {
  maxWidth: 640,
  margin: "0 auto",
  padding: "120px 24px",
  color: "#fff",
  textAlign: "center",
};
const TITLE: React.CSSProperties = {
  fontSize: 32,
  fontWeight: 800,
  margin: "0 0 12px",
};
const SUB: React.CSSProperties = {
  fontSize: 16,
  color: "rgba(255,255,255,.7)",
  margin: "0 0 24px",
};
const LINK: React.CSSProperties = {
  color: "#ff2d55",
  fontWeight: 600,
  textDecoration: "none",
};
