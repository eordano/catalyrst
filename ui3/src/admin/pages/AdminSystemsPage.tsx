import type { ComponentType } from "react";

import type {
  AdminLinkProps,
  AdminSystemsPageProps,
  DeployAction,
  ExperimentRow,
  Panel,
  SystemLink,
  SystemProbe,
  SystemUnit,
} from "./AdminSystemsTypes";
import "./adminsystems.css";

function PlainLink({ to, children, className }: AdminLinkProps) {
  return (
    <a href={to} className={className}>
      {children}
    </a>
  );
}

function Unavailable({ message, fix }: { message: string; fix?: string }) {
  return (
    <div className="adsys-empty" role="status">
      <p className="adsys-empty-msg">{message}</p>
      {fix ? <p className="adsys-empty-fix">Fix: {fix}</p> : null}
    </div>
  );
}

function Section<T>({
  title,
  panel,
  children,
}: {
  title: string;
  panel: Panel<T>;
  children: (data: T) => React.ReactNode;
}) {
  return (
    <section className="adsys-section">
      <h2 className="adsys-h2">{title}</h2>
      {panel.ok ? (
        children(panel.data)
      ) : (
        <Unavailable message={panel.message} fix={panel.fix} />
      )}
    </section>
  );
}

function unitTone(state: string): string {
  if (state === "active") return "ok";
  if (state === "activating" || state === "reloading") return "warn";
  if (state === "inactive") return "muted";
  return "bad";
}

function relTime(iso: string, now: number): string {
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return "\u{2014}";
  const s = Math.max(0, Math.round((now - t) / 1000));
  if (s < 60) return `${s}s ago`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.round(m / 60);
  if (h < 48) return `${h}h ago`;
  return `${Math.round(h / 24)}d ago`;
}

function Units({ units, now }: { units: SystemUnit[]; now: number }) {
  const down = units.filter((u) => u.active_state !== "active").length;
  return (
    <>
      <p className="adsys-note">
        {units.length} units &#xB7; {units.length - down} active &#xB7; {down} not active
      </p>
      <div className="adsys-scroll">
        <table className="adsys-table">
          <thead>
            <tr>
              <th>Unit</th>
              <th>State</th>
              <th className="adsys-num">Restarts</th>
              <th>Active since</th>
            </tr>
          </thead>
          <tbody>
            {units.map((u) => (
              <tr key={u.unit}>
                <td className="adsys-mono">{u.unit.replace(/\.service$/, "")}</td>
                <td>
                  <span className={`adsys-badge adsys-${unitTone(u.active_state)}`}>
                    {u.active_state}
                    {u.sub_state ? ` \u{B7} ${u.sub_state}` : ""}
                  </span>
                </td>
                <td className="adsys-num">
                  <span className={u.n_restarts > 0 ? "adsys-warn-text" : ""}>
                    {u.n_restarts}
                  </span>
                </td>
                <td className="adsys-dim">
                  {u.active_state === "active" ? relTime(u.active_since, now) : "\u{2014}"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}

function Probes({ probes }: { probes: SystemProbe[] }) {
  if (probes.length === 0) return null;
  return (
    <div className="adsys-probes">
      {probes.map((p) => (
        <a
          key={p.name}
          className={`adsys-probe adsys-${p.ok ? "ok" : "bad"}`}
          href={p.url}
          title={`${p.url} \u{2192} ${p.http_status || "no answer"}`}
        >
          <span className="adsys-dot" aria-hidden="true" />
          {p.name}
          <span className="adsys-probe-code">{p.http_status || "\u{D7}"}</span>
        </a>
      ))}
    </div>
  );
}

function Links({
  links,
  LinkComponent = PlainLink,
}: {
  links: SystemLink[];
  LinkComponent?: ComponentType<AdminLinkProps>;
}) {
  const scopes: ("public" | "operator")[] = ["public", "operator"];
  return (
    <div className="adsys-links">
      {scopes.map((scope) => {
        const group = links.filter((l) => l.scope === scope);
        if (group.length === 0) return null;
        return (
          <div key={scope} className="adsys-linkgroup">
            <h3 className="adsys-h3">{scope}</h3>
            <ul className="adsys-linklist">
              {group.map((l) => {
                const external = /^https?:\/\//.test(l.href);
                return (
                  <li key={l.href}>
                    {external ? (
                      <a href={l.href} className="adsys-link">
                        {l.label}
                      </a>
                    ) : (
                      <LinkComponent to={l.href} className="adsys-link">
                        {l.label}
                      </LinkComponent>
                    )}
                  </li>
                );
              })}
            </ul>
          </div>
        );
      })}
    </div>
  );
}

function Actions({ actions, now }: { actions: DeployAction[]; now: number }) {
  if (actions.length === 0) {
    return <p className="adsys-note">No deployments recorded.</p>;
  }
  return (
    <div className="adsys-scroll">
      <table className="adsys-table">
        <thead>
          <tr>
            <th>Type</th>
            <th>Entity</th>
            <th>Deployer</th>
            <th>When</th>
          </tr>
        </thead>
        <tbody>
          {actions.map((a) => (
            <tr key={a.entityId}>
              <td>{a.entityType}</td>
              <td className="adsys-mono adsys-ellipsis">{a.entityId}</td>
              <td className="adsys-mono adsys-ellipsis">{a.deployer}</td>
              <td className="adsys-dim">{relTime(a.at, now)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function metricLine(row: ExperimentRow): string {
  if (row.metrics.length === 0) return "no conversions yet";
  return row.metrics.map((m) => `${m.event}: ${m.count}`).join(" \u{B7} ");
}

function Experiments({ readable, unreadable }: { readable: ExperimentRow[]; unreadable: ExperimentRow[] }) {
  return (
    <>
      {readable.length === 0 ? (
        <p className="adsys-note">No experiment has recorded exposures yet.</p>
      ) : (
        <ul className="adsys-exps">
          {readable.map((e) => (
            <li key={e.exp_key} className="adsys-exp">
              <div className="adsys-exp-head">
                <span className="adsys-mono">{e.exp_key}</span>
                <span className="adsys-badge adsys-ok">{e.exposures} exposed</span>
              </div>
              <div className="adsys-exp-meta">
                <span>
                  {e.variants.length} variants: {e.variants.join(", ")}
                  {e.control ? ` (control: ${e.control})` : ""}
                </span>
                <span className="adsys-dim">{metricLine(e)}</span>
              </div>
            </li>
          ))}
        </ul>
      )}
      {unreadable.length > 0 ? (
        <details className="adsys-details">
          <summary>{unreadable.length} not yet readable</summary>
          <ul className="adsys-exps">
            {unreadable.map((e) => (
              <li key={e.exp_key} className="adsys-exp adsys-exp-muted">
                <span className="adsys-mono">{e.exp_key}</span>
                <span className="adsys-dim">{e.reason ?? "unreadable"}</span>
              </li>
            ))}
          </ul>
        </details>
      ) : null}
    </>
  );
}

export default function AdminSystemsPage({
  systems,
  links,
  actions,
  experiments,
  now,
  LinkComponent,
}: AdminSystemsPageProps) {
  return (
    <main className="adsys">
      <header className="adsys-header">
        <h1 className="adsys-h1">Operations</h1>
        {systems.ok ? (
          <p className={`adsys-freshness ${systems.data.stale ? "adsys-stale" : ""}`}>
            Snapshot {relTime(systems.data.collectedAt, now)}
            {systems.data.stale ? " \u{B7} stale" : ""}
          </p>
        ) : null}
      </header>

      <Section title="Live health" panel={systems}>
        {(data) => <Probes probes={data.probes} />}
      </Section>

      <Section title="Systemd units" panel={systems}>
        {(data) => <Units units={data.units} now={now} />}
      </Section>

      <section className="adsys-section">
        <h2 className="adsys-h2">Deployed surfaces</h2>
        <Links links={links} LinkComponent={LinkComponent} />
      </section>

      <Section title="Latest deployments" panel={actions}>
        {(rows) => <Actions actions={rows} now={now} />}
      </Section>

      <Section title="Experiments" panel={experiments}>
        {(data) => (
          <Experiments readable={data.readable} unreadable={data.unreadable} />
        )}
      </Section>
    </main>
  );
}
