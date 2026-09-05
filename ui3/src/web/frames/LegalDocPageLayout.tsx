import type { CSSProperties, MouseEvent, ReactNode } from "react";
import { SitesChromeMaybe, type SitesNavId } from "./SitesChrome";
import { SIDEBAR_PAGES, LEGAL_ICON_PATHS } from "../../data/legalPageConfig";
import "./legaldoc.css";

export type TocItem = { id: string; label: string; depth?: number };

export type LegalBlock =
  | string
  | {
      type?: string;
      id?: string;
      content?: ReactNode;
      items?: ReactNode[];
    };

export type LegalSection = { id: string; heading: string; body?: LegalBlock[] };

export type LegalDoc = {
  title?: string;
  activeSlug?: string;
  chromeActive?: SitesNavId;
  tableOfContents?: TocItem[];
  intro?: ReactNode;
  sections?: LegalSection[];
};

type SidebarPage = { label: string; slug: string; icon: string };

const ICON_PATHS: Record<string, string | undefined> = LEGAL_ICON_PATHS;

function SidebarIcon({ name }: { name: string }) {
  return (
    <svg className="legaldoc__icon" viewBox="0 0 24 24" aria-hidden="true">
      <path d={ICON_PATHS[name]} fill="currentColor" />
    </svg>
  );
}

function Block({ block }: { block: LegalBlock; i: number }) {
  const b = typeof block === "string" ? { type: "p", content: block } : block;
  switch (b.type) {
    case "h3":
      return (
        <h3 className="legaldoc__h3" id={b.id}>
          {b.content}
        </h3>
      );
    case "ul":
      return (
        <ul className="legaldoc__list">
          {(b.items ?? []).map((item, j) => (
            <li key={j}>{item}</li>
          ))}
        </ul>
      );
    case "node":
      return b.content;
    case "p":
    default:
      return <p className="legaldoc__p">{b.content}</p>;
  }
}

type LegalDocPageLayoutProps = {
  chrome?: boolean;
  doc?: LegalDoc;
  title?: string;
  activeSlug?: string;
  tableOfContents?: TocItem[];
  intro?: ReactNode;
  sections?: LegalSection[];
  children?: ReactNode;
  sidebarPages?: SidebarPage[];
  chromeActive?: SitesNavId;
  onNavClick?: (e: MouseEvent<HTMLAnchorElement>, page: SidebarPage) => void;
};

export default function LegalDocPageLayout({
  chrome = true,
  doc,
  title,
  activeSlug,
  tableOfContents,
  intro,
  sections,
  children,
  sidebarPages = SIDEBAR_PAGES,
  chromeActive,
  onNavClick,
}: LegalDocPageLayoutProps) {
  const d: LegalDoc = doc ?? {};
  title = title !== undefined ? title : d.title;
  activeSlug = activeSlug !== undefined ? activeSlug : d.activeSlug;
  tableOfContents =
    tableOfContents !== undefined ? tableOfContents : d.tableOfContents ?? [];
  intro = intro !== undefined ? intro : d.intro;
  sections = sections !== undefined ? sections : d.sections ?? [];
  chromeActive = chromeActive !== undefined ? chromeActive : d.chromeActive;

  return (
    <SitesChromeMaybe chrome={chrome} active={chromeActive ?? "legal"} overlayNav>
      <div className="legaldoc">
        <div className="legaldoc__grid">
          <nav className="legaldoc__sidebar" aria-label="Legal documents">
            {sidebarPages.map((page) => {
              const active = page.slug === activeSlug;
              return (
                <a
                  key={page.slug}
                  href={page.slug}
                  className={"legaldoc__sidelink" + (active ? " is-active" : "")}
                  aria-current={active ? "page" : undefined}
                  onClick={onNavClick ? (e) => onNavClick(e, page) : undefined}
                >
                  <SidebarIcon name={page.icon} />
                  {page.label}
                </a>
              );
            })}
          </nav>

          <div className="legaldoc__content">
            {title ? <h1 className="legaldoc__title">{title}</h1> : null}

            {tableOfContents.length > 0 && (
              <ul className="legaldoc__toc">
                {tableOfContents.map((item) => (
                  <li
                    key={item.id}
                    style={
                      item.depth ? ({ paddingLeft: `${item.depth}rem` } as CSSProperties) : undefined
                    }
                  >
                    <a href={`#${item.id}`}>{item.label}</a>
                  </li>
                ))}
              </ul>
            )}

            {intro != null &&
              (typeof intro === "string" ? (
                <p className="legaldoc__p">{intro}</p>
              ) : (
                intro
              ))}

            {sections.map((section) => (
              <section
                key={section.id}
                id={section.id}
                className="legaldoc__section"
              >
                {section.heading ? (
                  <h2 className="legaldoc__h2">{section.heading}</h2>
                ) : null}
                {(section.body ?? []).map((block, i) => (
                  <Block key={i} block={block} i={i} />
                ))}
              </section>
            ))}

            {children}
          </div>
        </div>
      </div>
    </SitesChromeMaybe>
  );
}
