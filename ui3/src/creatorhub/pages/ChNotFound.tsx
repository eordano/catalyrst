import type { CSSProperties } from "react";
import Button from "../../atoms/Button";
import Spinner from "../../atoms/Spinner";
import { asset } from "../../asset";
import "./chnotfound.css";

const HERO_STYLE: CSSProperties & { "--bdnotfound-hero": string } = {
  "--bdnotfound-hero": `url(${asset("assets/dev-hero.webp")})`,
};

type ChNotFoundProps = {
  loading?: boolean;
  bare?: boolean;
  subtitle?: string;
  backLabel?: string;
  onBack?: () => void;
};

export default function ChNotFound({
  loading = false,
  subtitle = "Sorry, we couldn't find the page you were looking for",
  backLabel = "Go back",
  onBack = undefined,
}: ChNotFoundProps) {
  if (loading) {
    const loadingBody = (
      <div className="bdnotfound--loading" style={HERO_STYLE} role="status" aria-live="polite">
        <div className="bdnotfound__loader">
          <Spinner size={58} />
          <span className="u-visually-hidden">Loading…</span>
        </div>
      </div>
    );
    return loadingBody;
  }

  const body = (
    <div className="bdnotfound" style={HERO_STYLE}>
      <div className="bdnotfound__center">
        <h1 className="bdnotfound__title">404</h1>
        <p className="bdnotfound__subtitle">{subtitle}</p>
        {onBack ? (
          <Button
            variant="primary"
            size="lg"
            className="bdnotfound__back"
            onClick={onBack}
          >
            {backLabel}
          </Button>
        ) : null}
      </div>
    </div>
  );

  return body;
}
