import AdControlNotice from "@ui/admin/pages/AdControlNotice";

import type {
  DebugConsoleData,
  BudgetSummary,
  EnvVar,
} from "@data/lib/catalyst/governance/debug-console";

export type DebugPanel = "Admin" | "Debug";

/**
 * Governance debug console.
 *
 * No `authorized` prop: both reads behind this page are public
 * (`catalyrst-governance/src/handlers/health.rs:3` and
 * `catalyrst-governance/src/handlers/read.rs:220` take no auth extractor),
 * so there is nothing to gate -- they render unconditionally as public data.
 *
 * No tool buttons: none of `BudgetsUpdate`, `BadgesAdmin`, `TriggerFunction`,
 * `Notifications`, `InvalidateCache` has a backing endpoint. A single
 * permanent unavailable state carries that fact (`control-availability.ts`
 * -> `debug.tools`) instead of buttons that would emit telemetry for a call
 * that never happens.
 *
 * A failed read is reported, never replaced: `health` and `budgets` are
 * `null` with a reason rather than a fixture standing in as a healthy status
 * for a node whose governance service is down.
 */
type Props = {
  data: DebugConsoleData;
  panel: DebugPanel;
  onPanelSwitch: (panel: DebugPanel) => void;
};

function num(n: number): string {
  return n.toLocaleString("en-US");
}

const wrap: React.CSSProperties = {
  maxWidth: 1440,
  margin: "0 auto",
  padding: "24px 24px 48px",
  display: "flex",
  flexDirection: "column",
  gap: 16,
  color: "rgba(22,22,22,0.86)",
};

const card: React.CSSProperties = {
  border: "1px solid rgba(22,22,22,0.12)",
  borderRadius: 12,
  background: "#fff",
  padding: "14px 16px",
};

export default function DebugConsolePanels({
  data,
  panel,
  onPanelSwitch,
}: Props) {
  return (
    <div className="adc">
      <div style={wrap}>
        <header style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <h1 style={{ margin: 0, fontSize: 22, fontWeight: 700 }}>
            Governance debug &amp; ops console
          </h1>
          <p style={{ margin: 0, color: "rgba(22,22,22,0.55)", fontSize: 14 }}>
            Read-only diagnostics for the DAO governance service. Version{" "}
            <strong>{data.version}</strong> &#xB7; health{" "}
            <HealthPill
              ok={data.health?.ok ?? false}
              status={data.health?.status ?? "unknown"}
              missing={data.health == null}
            />
          </p>
          <p style={{ margin: 0, color: "rgba(22,22,22,0.55)", fontSize: 13 }}>
            <strong>Public data &#x2014; no authorization required.</strong>{" "}
            <code>GET /governance/health</code> and{" "}
            <code>GET /governance/budgets</code> take no auth extractor
            (<code>health.rs:3</code>, <code>read.rs:220</code>). Anyone, signed
            in or not, gets exactly this page.
          </p>
        </header>

        <div
          role="group"
          aria-label="Console panel"
          style={{ display: "flex", gap: 10 }}
        >
          {(["Admin", "Debug"] as DebugPanel[]).map((p) => (
            <a
              key={p}
              href={`?panel=${p}`}
              onClick={(e) => {
                e.preventDefault();
                onPanelSwitch(p);
              }}
              aria-current={p === panel ? "true" : undefined}
              style={{
                padding: "8px 16px",
                borderRadius: 8,
                fontWeight: 600,
                textDecoration: "none",
                border: "1px solid rgba(22,22,22,0.16)",
                background: p === panel ? "#ff2d55" : "#fff",
                color: p === panel ? "#fff" : "rgba(22,22,22,0.8)",
              }}
            >
              {p}
            </a>
          ))}
        </div>

        {panel === "Admin" ? (
          <BudgetsSection budgets={data.budgets} reason={data.budgetsReason} />
        ) : (
          <DebugReads
            env={data.env}
            snapshot={data.snapshot}
            health={data.health}
            healthReason={data.healthReason}
          />
        )}

        <AdControlNotice
          title="Privileged tooling"
          message={data.tools.message}
          serverCheck={data.tools.serverCheck}
          fix={data.tools.fix}
        />
      </div>
    </div>
  );
}

function HealthPill({
  ok,
  status,
  missing,
}: {
  ok: boolean;
  status: string;
  missing: boolean;
}) {
  const good = ok && !missing;
  return (
    <span
      style={{
        display: "inline-block",
        padding: "1px 8px",
        borderRadius: 999,
        fontSize: 12,
        fontWeight: 600,
        background: good ? "#e6f7ec" : "#fde9ec",
        color: good ? "#157347" : "#d80027",
      }}
    >
      {missing ? "unknown" : good ? "ok" : status}
    </span>
  );
}

function BudgetsSection({
  budgets,
  reason,
}: {
  budgets: BudgetSummary[] | null;
  reason: string | null;
}) {
  return (
    <section style={card}>
      <h2 style={{ margin: "0 0 8px", fontSize: 16 }}>
        Transparency budgets{" "}
        <span style={{ color: "rgba(22,22,22,0.45)", fontWeight: 400 }}>
          (GET /governance/budgets &#x2014; public)
        </span>
      </h2>
      {budgets == null ? (
        <p style={{ margin: 0, color: "#a1001f" }} role="status">
          Budgets unavailable &#x2014; {reason ?? "no reason reported"}. No figures are
          shown; the bundled fixture that used to stand in here has been
          removed.
        </p>
      ) : (
        <ul style={{ margin: 0, paddingLeft: 18, lineHeight: 1.7 }}>
          {budgets.map((b) => (
            <li key={`${b.start}-${b.finish}`}>
              <strong>
                {b.start} &#x2192; {b.finish}
              </strong>{" "}
              &#x2014; ${num(b.total)}
              {b.categories.length > 0 && (
                <span style={{ color: "rgba(22,22,22,0.55)" }}>
                  {" "}
                  &#xB7;{" "}
                  {b.categories
                    .slice(0, 3)
                    .map((c) => `${c.name} ${c.pct}%`)
                    .join(", ")}
                </span>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function DebugReads({
  env,
  snapshot,
  health,
  healthReason,
}: {
  env: EnvVar[];
  snapshot: DebugConsoleData["snapshot"];
  health: DebugConsoleData["health"];
  healthReason: string | null;
}) {
  return (
    <>
      <section style={card}>
        <h2 style={{ margin: "0 0 8px", fontSize: 16 }}>
          Frontend env variable names{" "}
          <span style={{ color: "rgba(22,22,22,0.45)", fontWeight: 400 }}>
            (page chrome &#x2014; descriptive labels, not read from this node)
          </span>
        </h2>
        <dl
          style={{
            margin: 0,
            display: "grid",
            gridTemplateColumns: "minmax(200px, max-content) 1fr",
            columnGap: 16,
            rowGap: 4,
          }}
        >
          {env.map((e) => (
            <div key={e.name} style={{ display: "contents" }}>
              <dt style={{ fontWeight: 600 }}>{e.name}</dt>
              <dd style={{ margin: 0, color: "rgba(22,22,22,0.65)" }}>
                <code>{e.value}</code>
              </dd>
            </div>
          ))}
        </dl>
      </section>

      <section style={card}>
        <h2 style={{ margin: "0 0 8px", fontSize: 16 }}>
          Snapshot space{" "}
          <span style={{ color: "rgba(22,22,22,0.45)", fontWeight: 400 }}>
            ({snapshot.space} &#xB7; network {snapshot.network})
          </span>
        </h2>
        <p style={{ margin: "0 0 4px", fontWeight: 600 }}>Config</p>
        <pre style={preStyle}>{snapshot.config}</pre>
        <p style={{ margin: "8px 0 4px", fontWeight: 600 }}>Space</p>
        <pre style={preStyle}>{snapshot.spaceInfo}</pre>
        <p style={{ margin: "8px 0 0", color: "rgba(22,22,22,0.55)" }}>
          {health ? (
            <>
              Service health: <code>{health.service}</code> &#x2192;{" "}
              <strong>{health.status}</strong> ({health.checkedPath})
            </>
          ) : (
            <>
              Service health unavailable &#x2014; {healthReason ?? "no reason reported"}
              .
            </>
          )}
        </p>
      </section>
    </>
  );
}

const preStyle: React.CSSProperties = {
  margin: 0,
  padding: "8px 10px",
  background: "rgba(22,22,22,0.04)",
  borderRadius: 8,
  overflowX: "auto",
  fontSize: 13,
};
