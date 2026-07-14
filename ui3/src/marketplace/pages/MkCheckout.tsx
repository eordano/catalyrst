import type { ReactNode } from "react";

import ChromeShell from "../../components/ChromeShell";
import DclTopBar from "../../web/frames/DclTopBar";
import { creditsNoun } from "../credits-unit";
import "./mkcheckoutpage.css";

export function MkCheckoutFrame({
  title = "Checkout",
  topbar,
  back,
  wide = false,
  children,
}: {
  title?: string;
  topbar?: ReactNode;
  back?: { href: string; label: string };
  wide?: boolean;
  children: ReactNode;
}) {
  return (
    <ChromeShell
      className="mk"
      ariaLabel={title}
      subnav={false}
      topbar={topbar ?? <DclTopBar variant="sites" active="shop" />}
    >
      <div className={"mkco" + (wide ? " mkco--wide" : "")}>
        <div className="mkco__titlerow">
          {back ? (
            <a className="mkco__backbtn" href={back.href} aria-label={back.label}>
              <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <path d="M12 4l-6 6 6 6" />
              </svg>
            </a>
          ) : null}
          <h1 className="mkco__title">{title}</h1>
        </div>
        {children}
      </div>
    </ChromeShell>
  );
}

type Tone = "default" | "success" | "error" | "info";

export function MkCheckoutCard({
  tone = "default",
  busy = false,
  children,
}: {
  tone?: Tone;
  busy?: boolean;
  children: ReactNode;
}) {
  return (
    <div
      className={"mkco__card" + (tone !== "default" ? ` mkco__card--${tone}` : "")}
      role={busy ? "status" : undefined}
      aria-busy={busy || undefined}
    >
      {children}
    </div>
  );
}

export function MkCheckoutActions({
  children,
  between = false,
}: {
  children: ReactNode;
  between?: boolean;
}) {
  return (
    <div className={"mkco__actions" + (between ? " mkco__actions--between" : "")}>
      {children}
    </div>
  );
}

export type MkCheckoutLine = {
  key: string;
  name: string;
  qty?: number;
  unitPriceCredits: string;
  thumbnail?: string | null;
  action?: ReactNode;
};

export function MkCheckoutSummary({
  heading,
  lines,
  total,
}: {
  heading: string;
  lines: MkCheckoutLine[];
  total?: string;
}) {
  return (
    <div className="mkco__summary">
      <div className="mkco__summaryhead">{heading}</div>
      {lines.map((l) => (
        <div className="mkco__line" key={l.key}>
          {l.thumbnail ? (
            <span className="mkco__thumb" aria-hidden="true">
              <img src={l.thumbnail} alt="" loading="lazy" />
            </span>
          ) : null}
          <span className="mkco__linename">
            {l.name}
            {l.qty && l.qty > 1 ? ` ×${l.qty}` : ""}
          </span>
          <span className="mkco__lineprice">{l.unitPriceCredits} {creditsNoun(l.unitPriceCredits, true)}</span>
          {l.action ? <span className="mkco__lineact">{l.action}</span> : null}
        </div>
      ))}
      {total != null ? (
        <div className="mkco__total">
          <span className="mkco__totallabel">Total</span>
          <span className="mkco__totalval">{total} {creditsNoun(total, true)}</span>
        </div>
      ) : null}
    </div>
  );
}
