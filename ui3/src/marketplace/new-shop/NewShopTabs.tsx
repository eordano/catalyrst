import type { KeyboardEvent } from "react";
import { useEffect, useRef } from "react";
import "./newshoptabs.css";

export type NewShopTab<Id extends string = string> = { id: Id; label: string; href?: string };

export const NEW_SHOP_TABS: NewShopTab<"overview" | "all-assets" | "my-assets" | "my-favorites">[] = [
  { id: "overview", label: "Overview" },
  { id: "all-assets", label: "All Assets" },
  { id: "my-assets", label: "My Assets" },
  { id: "my-favorites", label: "My Favorites" },
];

type NewShopTabsProps<Id extends string> = {
  tabs: readonly NewShopTab<Id>[];
  active?: NoInfer<Id>;
  onTab?: (id: NoInfer<Id>) => void;
  ariaLabel?: string;
  className?: string;
};

export default function NewShopTabs<Id extends string = string>({
  tabs,
  active,
  onTab,
  ariaLabel = "Shop sections",
  className = "",
}: NewShopTabsProps<Id>) {
  const refs = useRef<(HTMLElement | null)[]>([]);
  const barRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const i = tabs.findIndex((t) => t.id === active);
    refs.current[i]?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, [active, tabs]);

  useEffect(() => {
    const el = barRef.current;
    if (!el) return;
    const update = () => {
      const canLeft = el.scrollLeft > 1;
      const canRight = el.scrollLeft + el.clientWidth < el.scrollWidth - 1;
      const fade = canLeft && canRight ? "both" : canLeft ? "left" : canRight ? "right" : "none";
      el.setAttribute("data-fade", fade);
    };
    update();
    el.addEventListener("scroll", update, { passive: true });
    window.addEventListener("resize", update);
    return () => {
      el.removeEventListener("scroll", update);
      window.removeEventListener("resize", update);
    };
  }, [tabs, active]);

  function onKeyDown(e: KeyboardEvent<HTMLElement>, index: number) {
    let next = index;
    if (e.key === "ArrowRight" || e.key === "ArrowDown") next = (index + 1) % tabs.length;
    else if (e.key === "ArrowLeft" || e.key === "ArrowUp") next = (index - 1 + tabs.length) % tabs.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = tabs.length - 1;
    else return;
    e.preventDefault();
    const t = tabs[next];
    if (!t) return;
    refs.current[next]?.focus();
    if (!t.href) onTab?.(t.id);
  }

  return (
    <div ref={barRef} className={"nstabs" + (className ? " " + className : "")} role="tablist" aria-label={ariaLabel}>
      {tabs.map((t, i) => {
        const is = t.id === active;
        const cls = "nstabs__tab" + (is ? " is-active" : "");
        if (t.href) {
          return (
            <a
              key={t.id}
              ref={(el) => {
                refs.current[i] = el;
              }}
              href={t.href}
              className={cls}
              tabIndex={is ? 0 : -1}
              onKeyDown={(e) => onKeyDown(e, i)}
            >
              {t.label}
            </a>
          );
        }
        return (
          <button
            key={t.id}
            ref={(el) => {
              refs.current[i] = el;
            }}
            type="button"
            role="tab"
            aria-selected={is}
            tabIndex={is ? 0 : -1}
            className={cls}
            onClick={() => onTab?.(t.id)}
            onKeyDown={(e) => onKeyDown(e, i)}
          >
            {t.label}
          </button>
        );
      })}
    </div>
  );
}
