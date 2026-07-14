import type { ReactNode } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import "./newshopfeaturedrow.css";

const Chevron = ({ dir }: { dir: "left" | "right" }) => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d={dir === "left" ? "M15 5l-7 7 7 7" : "M9 5l7 7-7 7"}
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

type NewShopFeaturedRowProps = {
  title: ReactNode;
  viewAllLabel?: ReactNode;
  onViewAll?: () => void;
  onPrev?: () => void;
  onNext?: () => void;
  children?: ReactNode;
};

export default function NewShopFeaturedRow({
  title,
  viewAllLabel = "View all",
  onViewAll,
  onPrev,
  onNext,
  children,
}: NewShopFeaturedRowProps) {
  const trackRef = useRef<HTMLDivElement>(null);
  const [atStart, setAtStart] = useState(true);
  const [atEnd, setAtEnd] = useState(false);

  const sync = useCallback(() => {
    const el = trackRef.current;
    if (!el) return;
    const max = el.scrollWidth - el.clientWidth;
    setAtStart(el.scrollLeft <= 1);
    setAtEnd(max <= 1 || el.scrollLeft >= max - 1);
  }, []);

  useEffect(() => {
    sync();
    const el = trackRef.current;
    if (!el) return;
    el.addEventListener("scroll", sync, { passive: true });
    window.addEventListener("resize", sync);
    return () => {
      el.removeEventListener("scroll", sync);
      window.removeEventListener("resize", sync);
    };
  }, [sync, children]);

  function scrollBy(dir: -1 | 1) {
    const el = trackRef.current;
    if (el) el.scrollBy({ left: dir * Math.max(240, el.clientWidth * 0.85), behavior: "smooth" });
  }

  return (
    <section className="nsfeat">
      <header className="nsfeat__head">
        <h3 className="nsfeat__title">{title}</h3>
        <div className="nsfeat__actions">
          {onViewAll ? (
            <button type="button" className="nsfeat__viewall" onClick={onViewAll}>
              {viewAllLabel}
            </button>
          ) : null}
          <div className="nsfeat__arrows">
            <button
              type="button"
              className="nsfeat__arrow"
              aria-label="Previous"
              disabled={atStart}
              onClick={() => {
                scrollBy(-1);
                onPrev?.();
              }}
            >
              <Chevron dir="left" />
            </button>
            <button
              type="button"
              className="nsfeat__arrow"
              aria-label="Next"
              disabled={atEnd}
              onClick={() => {
                scrollBy(1);
                onNext?.();
              }}
            >
              <Chevron dir="right" />
            </button>
          </div>
        </div>
      </header>
      <div className="nsfeat__track" ref={trackRef}>
        {children}
      </div>
    </section>
  );
}
