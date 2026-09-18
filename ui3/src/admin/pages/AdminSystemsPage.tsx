import type { ComponentType } from "react";

import EmptyState from "../../components/EmptyState";
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
import "../admin.css";
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
    <EmptyState
      variant="inline"
      titleAs="p"
      title={message}
      subtitle={fix ? `Fix: ${fix}` : undefined}
      role="status"
    />
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
    <section className="adm-card">
      <h2 className="adm__h2">{title}</h2>
      {panel.ok ? (
        children(panel.data)
      ) : (
        <Unavailable message={panel.message} fix={panel.fix} />
      )}
    </section>
  );
}

function unitTone(state: string): "ok" | "warn" | "bad" | undefined {
  if (state === "active") return "ok";
  if (state === "activating" || state === "reloading") return "warn";
  if (state === "inactive") return undefined;
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
      <p className="adm-card__text">
        {units.length} units &#xB7; {units.length - down} active &#xB7; {down} not active
      </p>
      <div className="adm-scroll">
        <table className="adm-table">
          <thead>
            <tr>
              <th>Unit</th>
              <th>State</th>
              <th className="is-num">Restarts</th>
              <th>Active since</th>
            </tr>
          </thead>
          <tbody>
            {units.map((u) => (
              <tr key={u.unit}>
                <td className="adm-mono">{u.unit.replace(/\.service$/, "")}</td>
                <td>
                  <span className="adm-status" data-tone={unitTone(u.active_state)}>
                    {u.active_state}
                    {u.sub_state ? ` \u{B7} ${u.sub_state}` : ""}
                  </span>
                </td>
                <td className="is-num">
                  <span className={u.n_restarts > 0 ? "adm-warn" : undefined}>
                    {u.n_restarts}
                  </span>
                </td>
                <td className="adm-dim is-nowrap">
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
    <div className="adm-pills">
      {probes.map((p) => (
        <a
          key={p.name}
          className="adm-status adm-status--dot"
          data-tone={p.ok ? "ok" : "bad"}
          href={p.url}
          title={`${p.url} \u{2192} ${p.http_status || "no answer"}`}
        >
          {p.name}
          <span className="adm-dim">{p.http_status || "\u{D7}"}</span>
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
    <div className="adm-grid">
      {scopes.map((scope) => {
        const group = links.filter((l) => l.scope === scope);
        if (group.length === 0) return null;
        return (
          <div key={scope} className="adm-stack">
            <h3 className="adm__h3">{scope}</h3>
            <ul className="adm-list">
              {group.map((l) => {
                const external = /^https?:\/\//.test(l.href);
                return (
                  <li key={l.href}>
                    {external ? (
                      <a href={l.href} className="adm-link">
                        {l.label}
                      </a>
                    ) : (
                      <LinkComponent to={l.href} className="adm-link">
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
    return <p className="adm-card__text">No deployments recorded.</p>;
  }
  return (
    <div className="adm-scroll">
      <table className="adm-table">
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
              <td className="adm-mono u-truncate adm-systems__id">{a.entityId}</td>
              <td className="adm-mono u-truncate adm-systems__id">{a.deployer}</td>
              <td className="adm-dim is-nowrap">{relTime(a.at, now)}</td>
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
        <p className="adm-card__text">No experiment has recorded exposures yet.</p>
      ) : (
        <ul className="adm-list">
          {readable.map((e) => (
            <li key={e.exp_key} className="adm-card">
              <div className="adm-card__head">
                <span className="adm-mono">{e.exp_key}</span>
                <span className="adm-status" data-tone="ok">
                  {e.exposures} exposed
                </span>
              </div>
              <p className="adm-card__text">
                {e.variants.length} variants: {e.variants.join(", ")}
                {e.control ? ` (control: ${e.control})` : ""}
              </p>
              <p className="adm-card__text adm-dim">{metricLine(e)}</p>
            </li>
          ))}
        </ul>
      )}
      {unreadable.length > 0 ? (
        <details className="adm-card__text adm-dim">
          <summary>{unreadable.length} not yet readable</summary>
          <ul className="adm-list">
            {unreadable.map((e) => (
              <li key={e.exp_key} className="adm-card">
                <div className="adm-card__head">
                  <span className="adm-mono">{e.exp_key}</span>
                  <span className="adm-dim">{e.reason ?? "unreadable"}</span>
                </div>
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
    <main className="adm">
      <div className="adm__page">
        <div className="adm__inner adm__inner--mid">
          <header className="adm__head">
            <h1 className="adm__title">Operations</h1>
            {systems.ok ? (
              <p className={systems.data.stale ? "adm__sub adm-warn" : "adm__sub"}>
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

          <section className="adm-card">
            <h2 className="adm__h2">Deployed surfaces</h2>
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
        </div>
      </div>
    </main>
  );
}
