import type { ComponentType } from "react";

import Button from "../../atoms/Button";
import EmptyState from "../../components/EmptyState";
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
import "../admin.css";

type Tone = "ok" | "warn" | "bad" | undefined;

function PlainForm({ method, children, className }: OperatorFormProps) {
  return (
    <form method={method} className={className}>
      {children}
    </form>
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

function ago(ms: number): string {
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s}s ago`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m}m ago`;
  return `${Math.round(m / 60)}h ago`;
}

function stateTone(s: ServerServiceRow["state"]): Tone {
  if (s === "ok") return "ok";
  if (s === "answering") return "warn";
  if (s === "off") return undefined;
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
    <ul className="adm-steps">
      {items.map((a) =>
        a.startsWith("$ ") ? (
          <li key={a}>
            <code>{a.slice(2)}</code>
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
    <li className="adm-card">
      <div className="adm-card__head">
        <div className="adm-row">
          <span className="adm-card__title">{s.name}</span>
          <span className="adm-mono adm-dim">
            {s.unit} &#xB7; :{s.port}
          </span>
        </div>
        <div className="adm-row">
          {s.state === "ok" && s.latencyMs > 0 ? (
            <span className="adm-dim">{s.latencyMs}ms</span>
          ) : null}
          {s.ageMs > 5000 ? <span className="adm-dim">checked {ago(s.ageMs)}</span> : null}
          {s.recovered ? (
            <span className="adm-status" data-tone="ok">
              recovered
            </span>
          ) : null}
          <span className="adm-status" data-tone={stateTone(s.state)}>
            {badgeText(s)}
          </span>
        </div>
      </div>
      <p className="adm-card__text adm-dim">{s.serves}</p>
      {unhealthy ? (
        <div
          className="adm-notice adm-server__remedy"
          data-tone={s.state === "down" ? "bad" : "warn"}
        >
          <div className="adm-actions adm-actions--split">
            <p aria-live="polite">{s.detail}</p>
            <FormComponent method="get">
              <input type="hidden" name="recheck" value={s.key} />
              <Button type="submit" variant="secondary" size="sm" disabled={rechecking}>
                {rechecking ? "Rechecking\u{2026}" : "Recheck now"}
              </Button>
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
      <p className="adm-card__text" aria-live="polite">
        {servicesSummary(enabled, watch)}
      </p>
      <ul className="adm-list">
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
        <details className="adm-card__text adm-dim">
          <summary>
            {off.length} {off.length === 1 ? "service" : "services"} not enabled on this
            node
          </summary>
          <ul className="adm-list">
            {off.map((s) => (
              <li key={s.key}>
                <span className="adm-mono">{s.unit}</span>
                <span className="adm-dim"> &#x2014; {s.serves}</span>
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
    <p className={low ? "adm-card__text adm-warn" : "adm-card__text"}>
      Disk ({disk.path}): {disk.usedPercent}% used &#xB7; {fmtBytes(disk.freeBytes)} free
      {low
        ? " \u{2014} free space now: a full disk takes PostgreSQL and every service down with it"
        : ""}
    </p>
  );
}

function envChips(row: ServerEnvRow): { label: string; tone: Tone }[] {
  const chips: { label: string; tone: Tone }[] = [];
  if (row.pendingRestart) chips.push({ label: "saved \u{2014} restart to apply", tone: "warn" });
  else if (row.liveInSites && row.fileValue === null)
    chips.push({ label: "set outside this file", tone: undefined });
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
    <li className="adm-card" id={`env-${row.name}`}>
      <div className="adm-card__head">
        <span className="adm-card__title adm-mono">{row.name}</span>
        <span className="adm-row">
          {envChips(row).map((c) => (
            <span key={c.label} className="adm-status" data-tone={c.tone}>
              {c.label}
            </span>
          ))}
        </span>
      </div>
      {row.purpose ? <p className="adm-card__text adm-dim">{row.purpose}</p> : null}
      <FormComponent method="post" className="adm-actions adm-actions--start">
        <input type="hidden" name="name" value={row.name} />
        <input
          className="adm-input adm-mono"
          type={row.secret ? "password" : "text"}
          name="value"
          defaultValue={row.secret ? "" : shown}
          placeholder={placeholder}
          autoComplete="off"
        />
        <Button
          type="submit"
          variant="secondary"
          name="intent"
          value="env-save"
          disabled={busy}
        >
          {envButtonLabel("Save", "Saving\u{2026}", row, "env-save", pendingEnv)}
        </Button>
        {row.fileValue !== null ? (
          <Button
            type="submit"
            variant="secondary"
            tone="danger"
            name="intent"
            value="env-delete"
            disabled={busy}
          >
            {envButtonLabel("Delete", "Deleting\u{2026}", row, "env-delete", pendingEnv)}
          </Button>
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
        <div className="adm-notice" data-tone={notice.ok ? "ok" : "bad"} role="status">
          <p>{notice.message}</p>
        </div>
      ) : null}
      <p className="adm-card__text">
        Persisted to <code>{env.data.path}</code>; services read it when they start, so
        restart a service to apply a change.
        {env.data.preservedLines > 0
          ? ` ${env.data.preservedLines} hand-written ${
              env.data.preservedLines === 1 ? "line" : "lines"
            } in the file ${env.data.preservedLines === 1 ? "is" : "are"} kept as-is.`
          : ""}
      </p>
      <ul className="adm-list">
        {env.data.rows.map((row) => (
          <EnvRowView
            key={row.name}
            row={row}
            pendingEnv={pendingEnv}
            FormComponent={FormComponent}
          />
        ))}
      </ul>
      <FormComponent method="post" className="adm-actions adm-actions--start">
        <input
          className="adm-input adm-mono"
          type="text"
          name="name"
          placeholder="NEW_VARIABLE"
          autoComplete="off"
        />
        <input
          className="adm-input adm-mono"
          type="text"
          name="value"
          placeholder="value"
          autoComplete="off"
        />
        <Button type="submit" variant="secondary" name="intent" value="env-save">
          Add
        </Button>
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
    <main className="adm">
      <div className="adm__page">
        <div className="adm__inner adm__inner--mid">
          <header className="adm__head">
            <h1 className="adm__title">Server</h1>
            <div className="adm-actions">
              {setupHref ? (
                <Button as="a" variant="secondary" href={setupHref}>
                  Setup guide
                </Button>
              ) : null}
              <FormComponent method="get">
                <Button type="submit" disabled={recheckingAll}>
                  {recheckingAll ? "Rechecking\u{2026}" : "Recheck all"}
                </Button>
              </FormComponent>
            </div>
          </header>

          {authMode === "edge" ? (
            <div className="adm-notice" data-tone="warn" role="note">
              <p>
                Access is controlled by the edge allowlist alone. Set <code>ADMIN_WALLETS</code>{" "}
                below to also require a signed-in operator wallet.
              </p>
            </div>
          ) : null}

          <section className="adm-card">
            <h2 className="adm__h2">Services</h2>
            {disk ? <DiskLine disk={disk} /> : null}
            <Services
              services={services}
              watch={watch}
              recheckingKey={recheckingKey}
              FormComponent={FormComponent}
            />
          </section>

          <section className="adm-card">
            <h2 className="adm__h2">Environment</h2>
            <EnvSection
              env={env}
              notice={notice}
              pendingEnv={pendingEnv}
              FormComponent={FormComponent}
            />
          </section>
        </div>
      </div>
    </main>
  );
}
