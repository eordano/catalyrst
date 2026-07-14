import type { ReactNode } from "react";
import "./newshopherobanner.css";

type NewShopHeroBannerProps = {
  eyebrow?: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  cta?: ReactNode;
  onCta?: () => void;
  art?: ReactNode;
  tone?: "purple" | "magenta" | "neon";
};

export default function NewShopHeroBanner({
  eyebrow,
  title,
  subtitle,
  cta,
  onCta,
  art,
  tone = "purple",
}: NewShopHeroBannerProps) {
  return (
    <section className={"nshero nshero--" + tone}>
      <div className="nshero__body">
        {eyebrow ? <span className="nshero__eyebrow">{eyebrow}</span> : null}
        <h2 className="nshero__title">{title}</h2>
        {subtitle ? <p className="nshero__subtitle">{subtitle}</p> : null}
        {cta ? (
          <button type="button" className="nshero__cta" onClick={onCta}>
            {cta}
          </button>
        ) : null}
      </div>
      {art ? (
        <div className="nshero__art" aria-hidden="true">
          {art}
        </div>
      ) : null}
    </section>
  );
}
