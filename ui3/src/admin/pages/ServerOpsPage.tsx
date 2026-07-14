import type { ComponentType } from "react";

import type {
  OperatorFormProps,
  ServerDisk,
  ServerEnvData,
  ServerEnvRow,
  ServerNotice,
  ServerOpsPageProps,
  ServerPanel,
  ServerPendingEnv,
  ServerServiceRow,
  ServerWatch,
} from "./ServerOpsTypes";
import "./serverops.css";

function PlainForm({ method, children, className }: OperatorFormProps) {
  return (
    <form method={method} className={className}>
      {children}
    </form>
  );
}

function Unavailable({ message, fix }: { message: string; fix?: string }) {
  return (
    <div className="srvops-empty" role="status">
      <p className="srvops-empty-msg">{message}</p>
      {fix ? <p className="srvops-empty-fix">Fix: {fix}</p> : null}
    </div>
  );
}

function ago(ms: number): string {
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s}s ago`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m}m ago`;
  return `${Math.round(m / 60)}h ago`;
}

function stateTone(s: ServerServiceRow["state"]): string {
  if (s === "ok") return "ok";
  if (s === "answering") return "warn";
  if (s === "off") return "muted";
  return "bad";
}

function badgeText(s: ServerServiceRow): string {
  if (s.state === "ok") return "up";
  if (s.state === "answering") return `answering \u{B7} HTTP ${s.httpStatus}`;
  return "down";
}

function Actionables({ items }: { items: string[] }) {
  if (items.length === 0) return null;
  return (
    <ul className="srvops-actionables">
      {items.map((a) =>
        a.startsWith("$ ") ? (
          <li key={a}>
            <code className="srvops-cmd">{a.slice(2)}</code>
          </li>
        ) : (
          <li key={a}>{a}</li>
        ),
      )}
    </ul>
  );
}

function ServiceCard({
  s,
  recheckingKey,
  FormComponent,
}: {
  s: ServerServiceRow;
  recheckingKey?: string | null;
  FormComponent: ComponentType<OperatorFormProps>;
}) {
  const unhealthy = s.state === "answering" || s.state === "down";
  const rechecking = recheckingKey === s.key;
  return (
    <li className={`srvops-service srvops-edge-${stateTone(s.state)}`}>
      <div className="srvops-service-head">
        <div className="srvops-service-id">
          <span className="srvops-service-name">{s.name}</span>
          <span className="srvops-mono srvops-dim">
            {s.unit} &#xB7; :{s.port}
          </span>
        </div>
        <div className="srvops-service-live">
          {s.state === "ok" && s.latencyMs > 0 ? (
            <span className="srvops-dim">{s.latencyMs}ms</span>
          ) : null}
          {s.ageMs > 5000 ? (
            <span className="srvops-dim">checked {ago(s.ageMs)}</span>
          ) : null}
          {s.recovered ? <span className="srvops-badge srvops-ok">recovered</span> : null}
          <span className={`srvops-badge srvops-${stateTone(s.state)}`}>{badgeText(s)}</span>
        </div>
      </div>
      <p className="srvops-serves">{s.serves}</p>
      {unhealthy ? (
        <div className="srvops-remedy">
          <div className="srvops-remedy-head">
            <p className="srvops-detail" aria-live="polite">
              {s.detail}
            </p>
            <FormComponent method="get" className="srvops-rowcheck">
              <input type="hidden" name="recheck" value={s.key} />
              <button className="srvops-btn" type="submit" disabled={rechecking}>
                {rechecking ? "Rechecking\u{2026}" : "Recheck now"}
              </button>
            </FormComponent>
          </div>
          <Actionables items={s.actionables} />
        </div>
      ) : null}
    </li>
  );
}

function servicesSummary(
  enabled: ServerServiceRow[],
  watch: ServerWatch | null | undefined,
): string {
  const down = enabled.filter((s) => s.state === "down").length;
  const answering = enabled.filter((s) => s.state === "answering").length;
  const up = enabled.length - down - answering;
  const parts = [`${up} of ${enabled.length} up`];
  if (answering > 0) parts.push(`${answering} answering with errors`);
  if (down > 0) parts.push(`${down} down`);
  if (watch && (down > 0 || answering > 0)) {
    parts.push(
      watch.checking
        ? "checking again now\u{2026}"
        : `rechecking every ${Math.round(watch.intervalMs / 1000)}s until they recover`,
    );
  }
  return parts.join(" \u{B7} ");
}

function Services({
  services,
  watch,
  recheckingKey,
  FormComponent,
}: {
  services: ServerServiceRow[];
  watch?: ServerWatch | null;
  recheckingKey?: string | null;
  FormComponent: ComponentType<OperatorFormProps>;
}) {
  const enabled = services.filter((s) => s.state !== "off");
  const off = services.filter((s) => s.state === "off");
  return (
    <>
      <p className="srvops-note" aria-live="polite">
        {servicesSummary(enabled, watch)}
      </p>
      <ul className="srvops-services">
        {enabled.map((s) => (
          <ServiceCard
            key={s.key}
            s={s}
            recheckingKey={recheckingKey}
            FormComponent={FormComponent}
          />
        ))}
      </ul>
      {off.length > 0 ? (
        <details className="srvops-off">
          <summary>
            {off.length} {off.length === 1 ? "service" : "services"} not enabled on this
            node
          </summary>
          <ul className="srvops-offlist">
            {off.map((s) => (
              <li key={s.key}>
                <span className="srvops-mono">{s.unit}</span>
                <span className="srvops-dim"> &#x2014; {s.serves}</span>
              </li>
            ))}
          </ul>
        </details>
      ) : null}
    </>
  );
}

function fmtBytes(n: number): string {
  if (n >= 1e12) return `${(n / 1e12).toFixed(1)} TB`;
  if (n >= 1e9) return `${(n / 1e9).toFixed(1)} GB`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(0)} MB`;
  return `${n} B`;
}

function DiskLine({ disk }: { disk: ServerDisk }) {
  const low = disk.usedPercent >= 90;
  return (
    <p className={`srvops-note ${low ? "srvops-warn-text" : ""}`}>
      Disk ({disk.path}): {disk.usedPercent}% used &#xB7; {fmtBytes(disk.freeBytes)} free
      {low
        ? " \u{2014} free space now: a full disk takes PostgreSQL and every service down with it"
        : ""}
    </p>
  );
}

function envChips(row: ServerEnvRow): { label: string; tone: string }[] {
  const chips: { label: string; tone: string }[] = [];
  if (row.pendingRestart) chips.push({ label: "saved \u{2014} restart to apply", tone: "warn" });
  else if (row.liveInSites && row.fileValue === null)
    chips.push({ label: "set outside this file", tone: "muted" });
  return chips;
}

function envButtonLabel(
  base: string,
  pendingLabel: string,
  row: ServerEnvRow,
  intent: ServerPendingEnv["intent"],
  pendingEnv?: ServerPendingEnv | null,
): string {
  return pendingEnv && pendingEnv.name === row.name && pendingEnv.intent === intent
    ? pendingLabel
    : base;
}

function EnvRowView({
  row,
  pendingEnv,
  FormComponent,
}: {
  row: ServerEnvRow;
  pendingEnv?: ServerPendingEnv | null;
  FormComponent: ComponentType<OperatorFormProps>;
}) {
  const shown = row.fileValue ?? row.liveValue ?? "";
  const placeholder = row.secret
    ? row.fileValue !== null || row.liveInSites
      ? "hidden \u{2014} enter a new value to replace"
      : "enter a value"
    : shown === ""
      ? "not set \u{2014} enter a value"
      : "value";
  const busy = pendingEnv?.name === row.name;
  return (
    <li className="srvops-envrow" id={`env-${row.name}`}>
      <div className="srvops-envrow-head">
        <span className="srvops-mono srvops-envname">{row.name}</span>
        <span className="srvops-chips">
          {envChips(row).map((c) => (
            <span key={c.label} className={`srvops-badge srvops-${c.tone}`}>
              {c.label}
            </span>
          ))}
        </span>
      </div>
      {row.purpose ? <p className="srvops-envpurpose">{row.purpose}</p> : null}
      <FormComponent method="post" className="srvops-envform">
        <input type="hidden" name="name" value={row.name} />
        <input
          className="srvops-input srvops-mono"
          type={row.secret ? "password" : "text"}
          name="value"
          defaultValue={row.secret ? "" : shown}
          placeholder={placeholder}
          autoComplete="off"
        />
        <button
          className="srvops-btn"
          name="intent"
          value="env-save"
          type="submit"
          disabled={busy}
        >
          {envButtonLabel("Save", "Saving\u{2026}", row, "env-save", pendingEnv)}
        </button>
        {row.fileValue !== null ? (
          <button
            className="srvops-btn srvops-btn-danger"
            name="intent"
            value="env-delete"
            type="submit"
            disabled={busy}
          >
            {envButtonLabel("Delete", "Deleting\u{2026}", row, "env-delete", pendingEnv)}
          </button>
        ) : null}
      </FormComponent>
    </li>
  );
}

function EnvSection({
  env,
  notice,
  pendingEnv,
  FormComponent,
}: {
  env: ServerPanel<ServerEnvData>;
  notice?: ServerNotice | null;
  pendingEnv?: ServerPendingEnv | null;
  FormComponent: ComponentType<OperatorFormProps>;
}) {
  if (!env.ok) return <Unavailable message={env.message} fix={env.fix} />;
  return (
    <>
      {notice ? (
        <p
          className={`srvops-banner ${notice.ok ? "srvops-banner-ok" : "srvops-banner-bad"}`}
          role="status"
        >
          {notice.message}
        </p>
      ) : null}
      <p className="srvops-note">
        Persisted to <code className="srvops-mono">{env.data.path}</code>; services read it
        when they start, so restart a service to apply a change.
        {env.data.preservedLines > 0
          ? ` ${env.data.preservedLines} hand-written ${
              env.data.preservedLines === 1 ? "line" : "lines"
            } in the file ${env.data.preservedLines === 1 ? "is" : "are"} kept as-is.`
          : ""}
      </p>
      <ul className="srvops-envlist">
        {env.data.rows.map((row) => (
          <EnvRowView
            key={row.name}
            row={row}
            pendingEnv={pendingEnv}
            FormComponent={FormComponent}
          />
        ))}
      </ul>
      <FormComponent method="post" className="srvops-envform srvops-envadd">
        <input
          className="srvops-input srvops-mono"
          type="text"
          name="name"
          placeholder="NEW_VARIABLE"
          autoComplete="off"
        />
        <input
          className="srvops-input srvops-mono"
          type="text"
          name="value"
          placeholder="value"
          autoComplete="off"
        />
        <button className="srvops-btn" name="intent" value="env-save" type="submit">
          Add
        </button>
      </FormComponent>
    </>
  );
}

export default function ServerOpsPage({
  services,
  env,
  authMode,
  setupHref,
  disk,
  notice,
  watch,
  recheckingAll,
  recheckingKey,
  pendingEnv,
  FormComponent = PlainForm,
}: ServerOpsPageProps) {
  return (
    <main className="srvops">
      <header className="srvops-header">
        <h1 className="srvops-h1">Server</h1>
        <div className="srvops-header-actions">
          {setupHref ? (
            <a className="srvops-btn" href={setupHref}>
              Setup guide
            </a>
          ) : null}
          <FormComponent method="get">
            <button
              className="srvops-btn srvops-btn-primary"
              type="submit"
              disabled={recheckingAll}
            >
              {recheckingAll ? "Rechecking\u{2026}" : "Recheck all"}
            </button>
          </FormComponent>
        </div>
      </header>

      {authMode === "edge" ? (
        <p className="srvops-banner" role="note">
          Access is controlled by the edge allowlist alone. Set{" "}
          <code className="srvops-mono">ADMIN_WALLETS</code> below to also require a
          signed-in operator wallet.
        </p>
      ) : null}

      <section className="srvops-section">
        <h2 className="srvops-h2">Services</h2>
        {disk ? <DiskLine disk={disk} /> : null}
        <Services
          services={services}
          watch={watch}
          recheckingKey={recheckingKey}
          FormComponent={FormComponent}
        />
      </section>

      <section className="srvops-section">
        <h2 className="srvops-h2">Environment</h2>
        <EnvSection
          env={env}
          notice={notice}
          pendingEnv={pendingEnv}
          FormComponent={FormComponent}
        />
      </section>
    </main>
  );
}
