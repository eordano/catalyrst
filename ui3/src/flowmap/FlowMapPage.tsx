import { createContext, useContext, useMemo, useState } from "react";
import type { ComponentType, ReactNode } from "react";

import type { FlowEdge, FlowNode, FlowSection, FlowStats, Track } from "./flowmapdata";
import "./flowmappage.css";


export type LinkComponentProps = {
  to: string;
  className?: string;
  title?: string;
  "aria-label"?: string;
  children?: ReactNode;
  onMouseEnter?: () => void;
  onMouseLeave?: () => void;
  onFocus?: () => void;
  onBlur?: () => void;
};

const LinkCtx = createContext<ComponentType<LinkComponentProps> | undefined>(undefined);


const ChainCtx = createContext<{
  active: string | null;
  setActive: (c: string | null) => void;
}>({ active: null, setActive: () => {} });

function useChain(chains: string[], demo?: boolean) {
  const { active, setActive } = useContext(ChainCtx);
  const primary = chains[0] ?? null;
  const lit = !demo && active !== null && chains.includes(active);
  const handlers =
    !demo && primary
      ? {
          onMouseEnter: () => setActive(primary),
          onMouseLeave: () => setActive(null),
          onFocus: () => setActive(primary),
          onBlur: () => setActive(null),
        }
      : {};
  return { lit, handlers };
}

const itemClass = (base: string, lit: boolean) =>
  `fm-item ${base}${lit ? " is-lit" : ""}`;


export function Hourglass({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      viewBox="0 0 10 12"
      width="9"
      height="11"
      aria-hidden="true"
      focusable="false"
    >
      <path
        d="M1 1h8M1 11h8M2 1c0 3 2.4 3.6 3 5-.6 1.4-3 2-3 5M8 1c0 3-2.4 3.6-3 5 .6 1.4 3 2 3 5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.2"
        strokeLinecap="round"
      />
    </svg>
  );
}

const Arrow = () => <i className="fm-arrow" aria-hidden="true" />;
const Seg = ({ dash, long }: { dash?: boolean; long?: boolean }) => (
  <i
    className={`fm-seg${dash ? " fm-seg--dash" : ""}${long ? " fm-seg--long" : ""}`}
    aria-hidden="true"
  />
);


function edgeAria(e: FlowEdge): string {
  switch (e.kind) {
    case "click":
      return `Click: ${e.label}`;
    case "load":
      return e.label
        ? `Click: ${e.label} — then a load over 100 milliseconds: ${e.work ?? "loading"}`
        : `Load over 100 milliseconds: ${e.work ?? "loading"}`;
    case "auto":
      return `Condition: ${e.label}`;
    case "reversible":
      return `Reversible click pair: ${e.label}`;
    case "step":
      return "Next step (one click)";
  }
}

function EdgeView({
  edge,
  chains,
  demo,
}: {
  edge: FlowEdge;
  chains: string[];
  demo?: boolean;
}) {
  const { lit, handlers } = useChain(chains, demo);
  const kind = edge.kind;

  return (
    <span
      className={itemClass(`fm-edge fm-edge--${kind}`, lit)}
      tabIndex={demo ? -1 : 0}
      aria-label={demo ? undefined : edgeAria(edge)}
      aria-hidden={demo || undefined}
      {...handlers}
    >
      {edge.label &&
        (kind === "auto" ? (
          <span className="fm-cond">{edge.label}</span>
        ) : kind === "reversible" ? (
          <span className="fm-key fm-key--rev">{edge.label}</span>
        ) : (
          <span className="fm-key">{edge.label}</span>
        ))}
      <span className="fm-wire" aria-hidden="true">
        {kind === "reversible" && <i className="fm-arrow fm-arrow--back" />}
        {kind === "load" ? (
          <>
            <Seg dash />
            <span className="fm-hg">
              <Hourglass />
            </span>
            <Seg dash />
          </>
        ) : (
          <Seg long={kind !== "step"} />
        )}
        <Arrow />
      </span>
      {kind === "load" && edge.work && <span className="fm-copy">{edge.work}</span>}
    </span>
  );
}


const KIND_NAME: Record<FlowNode["kind"], string> = {
  route: "Route",
  state: "Machine state",
  modal: "Modal",
  outcome: "Outcome",
  chip: "Affordance",
  external: "External tab",
  jump: "Jump to",
  end: "Terminus",
  sep: "",
};

function NodeView({
  node,
  chains,
  demo,
}: {
  node: FlowNode;
  chains: string[];
  demo?: boolean;
}) {
  const { lit, handlers } = useChain(chains, demo);
  const LinkComponent = useContext(LinkCtx);

  if (node.kind === "sep") {
    return (
      <span className="fm-item fm-sepdot" aria-hidden="true">
        ·
      </span>
    );
  }
  if (node.kind === "end") {
    return (
      <span
        className={itemClass("fm-node fm-node--end", lit)}
        aria-label="outcome as in product"
        title="outcome as in product"
        {...handlers}
      />
    );
  }

  const aria = `${KIND_NAME[node.kind]}: ${node.label}${
    node.href && node.kind !== "jump" ? ` — opens ${node.href}` : ""
  }`;
  const cls = itemClass(`fm-node fm-node--${node.kind}`, lit);
  const body = (
    <>
      <span className="fm-label">
        {node.busy && (
          <span className="fm-hg fm-hg--in" aria-hidden="true">
            <Hourglass />
          </span>
        )}
        {node.label}
        {node.kind === "external" && <span className="fm-ext" aria-hidden="true"> ↗</span>}
        {node.kind === "jump" && <span className="fm-ext" aria-hidden="true"> »</span>}
      </span>
      {node.sub && <span className="fm-sub">{node.sub}</span>}
    </>
  );

  if (node.href && node.href.startsWith("#")) {
    return (
      <a className={cls} href={node.href} aria-label={aria} title={node.title} {...handlers}>
        {body}
      </a>
    );
  }
  if (node.href) {
    if (LinkComponent && !node.plain) {
      return (
        <LinkComponent
          className={cls}
          to={node.href}
          aria-label={aria}
          title={node.title ?? node.href}
          {...handlers}
        >
          {body}
        </LinkComponent>
      );
    }
    return (
      <a
        className={cls}
        href={node.href}
        aria-label={aria}
        title={node.title ?? node.href}
        {...handlers}
      >
        {body}
      </a>
    );
  }
  return (
    <span
      className={cls}
      tabIndex={demo ? -1 : 0}
      aria-label={demo ? undefined : aria}
      aria-hidden={demo || undefined}
      title={node.title}
      {...handlers}
    >
      {body}
    </span>
  );
}


function TrackView({ track, depth = 0 }: { track: Track; depth?: number }) {
  return (
    <div className={`fm-trackrow${depth > 0 ? " fm-trackrow--branch" : ""}`}>
      <div className={`fm-track${track.chips ? " fm-track--chips" : ""}`}>
        {track.items.map((it, i) => {
          const chains = [
            ...(it.chains ?? []),
            ...(track.chain ? [track.chain] : []),
          ];
          const el =
            it.t === "node" ? (
              <NodeView key={i} node={it} chains={chains} />
            ) : (
              <EdgeView key={i} edge={it} chains={chains} />
            );
          if (track.chips && i > 0) {
            return (
              <span key={i} className="fm-chipcell">
                <span className="fm-sepdot" aria-hidden="true">
                  ·
                </span>
                {el}
              </span>
            );
          }
          return el;
        })}
        {track.note && <span className="fm-note">— {track.note}</span>}
      </div>
      {track.branches && track.branches.length > 0 && (
        <div className="fm-branches">
          {track.branches.map((b, i) => (
            <TrackView key={i} track={b} depth={depth + 1} />
          ))}
        </div>
      )}
    </div>
  );
}


function SectionView({
  section,
  machineTitle,
}: {
  section: FlowSection;
  machineTitle?: (m: string) => string;
}) {
  return (
    <section className="fm-section" id={section.id} aria-labelledby={`${section.id}-h`}>
      <span className="fm-ghostnum" aria-hidden="true">
        {section.num}
      </span>
      <header className="fm-sechead">
        <span className="fm-secnum">{section.num}</span>
        <h2 id={`${section.id}-h`}>{section.title}</h2>
        {section.machines && (
          <span className="fm-machines">
            {section.machines.map((m) => (
              <code key={m} title={machineTitle ? machineTitle(m) : m}>
                {m}
              </code>
            ))}
          </span>
        )}
      </header>
      <p className="fm-blurb">{section.blurb}</p>
      <div className="fm-tracks">
        {section.tracks.map((t, i) => (
          <TrackView key={i} track={t} />
        ))}
      </div>
    </section>
  );
}


function Legend() {
  return (
    <div className="fm-legend" aria-label="Legend">
      <div className="fm-legend-item">
        <EdgeView edge={{ t: "edge", kind: "click", label: "click" }} chains={[]} demo />
        <span>one user click</span>
      </div>
      <div className="fm-legend-item">
        <EdgeView
          edge={{ t: "edge", kind: "load", label: "click", work: "“Loading…”" }}
          chains={[]}
          demo
        />
        <span>
          click, then <em>&gt;100&thinsp;ms</em> of invoked work — the spinner copy shown
          in-product
        </span>
      </div>
      <div className="fm-legend-item">
        <EdgeView edge={{ t: "edge", kind: "auto", label: "condition" }} chains={[]} demo />
        <span>automatic — no click</span>
      </div>
      <div className="fm-legend-item">
        <EdgeView edge={{ t: "edge", kind: "reversible", label: "toggle" }} chains={[]} demo />
        <span>reversible pair</span>
      </div>
      <div className="fm-legend-item">
        <NodeView node={{ t: "node", kind: "route", label: "/route" }} chains={[]} demo />
        <span>route — click to open it</span>
      </div>
      <div className="fm-legend-item">
        <NodeView node={{ t: "node", kind: "state", label: "STATE" }} chains={[]} demo />
        <span>machine state</span>
      </div>
      <div className="fm-legend-item">
        <NodeView node={{ t: "node", kind: "modal", label: "MODAL" }} chains={[]} demo />
        <span>overlay</span>
      </div>
      <div className="fm-legend-item">
        <NodeView node={{ t: "node", kind: "external", label: "external" }} chains={[]} demo />
        <span>external tab</span>
      </div>
    </div>
  );
}


function AsciiDetails({ source, label }: { source: string; label: string }) {
  const [copied, setCopied] = useState(false);
  const copy = () => {
    if (typeof navigator !== "undefined" && navigator.clipboard) {
      navigator.clipboard.writeText(source).then(
        () => {
          setCopied(true);
          setTimeout(() => setCopied(false), 1600);
        },
        () => {},
      );
    }
  };
  return (
    <details className="fm-ascii">
      <summary>View as text — the ascii original, for copy-paste</summary>
      <div className="fm-ascii-body">
        <button type="button" className="fm-copybtn" onClick={copy}>
          {copied ? "Copied ✓" : "Copy"}
        </button>
        <pre tabIndex={0} aria-label={label}>
          {source}
        </pre>
      </div>
    </details>
  );
}


export type FlowMapCopy = {
  backHref: string;
  backLabel: string;
  backPlain?: boolean;
  crumb: string;
  lede: ReactNode;
  machineTitle?: (m: string) => string;
  asciiLabel: string;
  honesty: ReactNode;
};

export type FlowMapPageProps = {
  LinkComponent?: ComponentType<LinkComponentProps>;
  sections: FlowSection[];
  stats: FlowStats;
  ascii: string;
  copy: FlowMapCopy;
};

export default function FlowMapPage({
  LinkComponent = undefined,
  sections,
  stats,
  ascii,
  copy,
}: FlowMapPageProps) {
  const [active, setActive] = useState<string | null>(null);
  const ctx = useMemo(() => ({ active, setActive }), [active]);

  return (
    <LinkCtx.Provider value={LinkComponent}>
    <ChainCtx.Provider value={ctx}>
      <div className={`fmp${active ? " fmp--dim" : ""}`}>
        <div className="fmp-bg" aria-hidden="true" />

        <header className="fm-hero">
          <div className="fm-hero-top">
            {LinkComponent && !copy.backPlain ? (
              <LinkComponent to={copy.backHref} className="fm-back">
                {copy.backLabel}
              </LinkComponent>
            ) : (
              <a href={copy.backHref} className="fm-back">
                {copy.backLabel}
              </a>
            )}
            <span className="fm-crumb">{copy.crumb}</span>
          </div>

          <p className="fm-eyebrow">Decentraland · interaction topology</p>
          <h1 className="fm-title">
            Every <span className="fm-title-click">click</span>. Every{" "}
            <span className="fm-title-wait">
              <Hourglass className="fm-title-hg" /> wait
            </span>
            . One map.
          </h1>
          <p className="fm-lede">{copy.lede}</p>

          <dl className="fm-stats">
            <div>
              <dd>{stats.routes}</dd>
              <dt>route nodes</dt>
            </div>
            <div>
              <dd>{stats.states}</dd>
              <dt>machine states</dt>
            </div>
            <div>
              <dd>{stats.clicks}</dd>
              <dt>click edges</dt>
            </div>
            <div>
              <dd className="fm-stat-load">{stats.loads}</dd>
              <dt>
                <Hourglass className="fm-inline-hg" /> loads &gt;100 ms
              </dt>
            </div>
          </dl>

          <Legend />
        </header>

        <nav className="fm-toc" aria-label="Flow families">
          {sections.map((s) => (
            <a key={s.id} href={`#${s.id}`}>
              <span>{s.num}</span> {s.title}
            </a>
          ))}
        </nav>

        <main className="fm-map">
          {sections.map((s) => (
            <SectionView key={s.id} section={s} machineTitle={copy.machineTitle} />
          ))}
        </main>

        <footer className="fm-foot">
          <AsciiDetails source={ascii} label={copy.asciiLabel} />
          <p className="fm-honesty">{copy.honesty}</p>
        </footer>
      </div>
    </ChainCtx.Provider>
    </LinkCtx.Provider>
  );
}
