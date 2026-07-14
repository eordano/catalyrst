import type { ReactNode } from "react";

import type {
  ServerSetupPageProps,
  SetupAnswers,
  SetupIssue,
  SetupProfile,
  SetupTls,
} from "./ServerSetupTypes";
import "./serversetup.css";

const PROFILES: { value: SetupProfile; label: string; detail: string }[] = [
  {
    value: "content-node",
    label: "Content node",
    detail: "Content + sync + postgres + a plain-HTTP LAN edge. No sibling services.",
  },
  {
    value: "full-realm",
    label: "Full realm",
    detail:
      "Adds comms (LiveKit, archipelago, pulse), the explore/create/social/data bundles, world storage and profile images behind a public TLS edge.",
  },
  {
    value: "public-gateway",
    label: "Public gateway",
    detail:
      "Full realm plus the per-service gateway subdomains, asset-bundle CDN, governance, presence, telemetry and the marketplace indexer.",
  },
];

const TLS_MODES: { value: SetupTls; label: string; detail: string; publicOnly?: boolean }[] = [
  {
    value: "acme-http01",
    label: "ACME HTTP-01",
    detail: "One multi-name certificate, no DNS API token; every name must resolve first.",
  },
  {
    value: "acme-dns01",
    label: "ACME DNS-01",
    detail: "A wildcard certificate through your DNS provider's API credentials.",
  },
  {
    value: "none",
    label: "None",
    detail: "Plain HTTP \u{2014} the LAN edge; there is no self-signed mode.",
  },
];

function issuesFor(issues: SetupIssue[], field: keyof SetupAnswers): string[] {
  return issues.filter((i) => i.field === field).map((i) => i.message);
}

function Field({
  label,
  hint,
  errors,
  children,
}: {
  label: string;
  hint?: string;
  errors: string[];
  children: ReactNode;
}) {
  return (
    <label className="srvsetup-field">
      <span className="srvsetup-label">{label}</span>
      {children}
      {hint && errors.length === 0 ? <span className="srvsetup-hint">{hint}</span> : null}
      {errors.map((e) => (
        <span key={e} className="srvsetup-error" role="alert">
          {e}
        </span>
      ))}
    </label>
  );
}

function TextInput({
  answers,
  field,
  onChange,
  placeholder,
  mono = true,
}: {
  answers: SetupAnswers;
  field: keyof SetupAnswers & string;
  onChange: ServerSetupPageProps["onChange"];
  placeholder?: string;
  mono?: boolean;
}) {
  return (
    <input
      className={`srvsetup-input ${mono ? "srvsetup-mono" : ""}`}
      type="text"
      value={answers[field] as string}
      placeholder={placeholder}
      autoComplete="off"
      onChange={(e) => onChange(field as never, e.target.value as never)}
    />
  );
}

export default function ServerSetupPage({
  answers,
  issues,
  output,
  onChange,
  serverHref,
}: ServerSetupPageProps) {
  const publicShape = answers.profile !== "content-node";
  const acme = answers.tls === "acme-http01" || answers.tls === "acme-dns01";
  const ready = issues.length === 0 && answers.domain.trim() !== "";

  return (
    <main className="srvsetup">
      <header className="srvsetup-header">
        <h1 className="srvsetup-h1">Set up this server</h1>
        <p className="srvsetup-sub">
          Answer below; the host configuration and secrets files update as you type.
          Already running? <a href={serverHref}>Operations live at /server</a>.
        </p>
      </header>

      <div className="srvsetup-columns">
        <div className="srvsetup-form">
          <section className="srvsetup-section">
            <h2 className="srvsetup-h2">Shape</h2>
            <div className="srvsetup-cards" role="radiogroup" aria-label="Deployment shape">
              {PROFILES.map((p) => (
                <label
                  key={p.value}
                  className={`srvsetup-card ${answers.profile === p.value ? "srvsetup-card-on" : ""}`}
                >
                  <input
                    type="radio"
                    name="profile"
                    value={p.value}
                    checked={answers.profile === p.value}
                    onChange={() => onChange("profile", p.value)}
                  />
                  <span className="srvsetup-card-label">{p.label}</span>
                  <span className="srvsetup-card-detail">{p.detail}</span>
                </label>
              ))}
            </div>
          </section>

          <section className="srvsetup-section">
            <h2 className="srvsetup-h2">Identity</h2>
            <Field
              label="Domain"
              hint={
                publicShape
                  ? "The public name this node answers on; every emitted URL derives from it."
                  : "A LAN host name or IP."
              }
              errors={issuesFor(issues, "domain")}
            >
              <TextInput
                answers={answers}
                field="domain"
                onChange={onChange}
                placeholder={publicShape ? "realm.example.org" : "node.home.arpa"}
              />
            </Field>
            <Field label="TLS" errors={issuesFor(issues, "tls")}>
              <select
                className="srvsetup-input"
                value={answers.tls}
                onChange={(e) => onChange("tls", e.target.value as SetupTls)}
              >
                {TLS_MODES.map((t) => (
                  <option key={t.value} value={t.value}>
                    {t.label} &#x2014; {t.detail}
                  </option>
                ))}
              </select>
            </Field>
            {acme ? (
              <Field
                label="ACME contact email"
                hint="Certificate expiry notices go here."
                errors={issuesFor(issues, "acmeEmail")}
              >
                <TextInput
                  answers={answers}
                  field="acmeEmail"
                  onChange={onChange}
                  placeholder="ops@example.org"
                />
              </Field>
            ) : null}
          </section>

          <section className="srvsetup-section">
            <h2 className="srvsetup-h2">Chain access</h2>
            <Field
              label="Ethereum RPC (content access checks)"
              hint="Leave empty to use the default public endpoint; point at your own node to avoid the shared one."
              errors={issuesFor(issues, "ethRpcUrl")}
            >
              <TextInput
                answers={answers}
                field="ethRpcUrl"
                onChange={onChange}
                placeholder="https://..."
              />
            </Field>
            {answers.profile === "public-gateway" ? (
              <>
                <Field
                  label="Ethereum archive RPC (marketplace indexer) -- required"
                  errors={issuesFor(issues, "squidEthRpc")}
                >
                  <TextInput
                    answers={answers}
                    field="squidEthRpc"
                    onChange={onChange}
                    placeholder="https://..."
                  />
                </Field>
                <Field
                  label="Polygon archive RPC (marketplace indexer) -- required"
                  errors={issuesFor(issues, "squidPolygonRpc")}
                >
                  <TextInput
                    answers={answers}
                    field="squidPolygonRpc"
                    onChange={onChange}
                    placeholder="https://..."
                  />
                </Field>
                <Field
                  label="SQD portal key (marketplace indexer) -- required"
                  hint="The polygon processor streams history through the authenticated SQD portal. Free key at sqd.ai."
                  errors={issuesFor(issues, "sqdPortalKey")}
                >
                  <TextInput
                    answers={answers}
                    field="sqdPortalKey"
                    onChange={onChange}
                    placeholder="sqd_..."
                  />
                </Field>
              </>
            ) : null}
          </section>

          <section className="srvsetup-section">
            <h2 className="srvsetup-h2">Access</h2>
            <Field
              label="Admin wallet addresses"
              hint="Gate the /admin console and /server operations page. Comma or space separated; empty keeps /server edge-gated only."
              errors={issuesFor(issues, "adminAddresses")}
            >
              <textarea
                className="srvsetup-input srvsetup-mono"
                rows={2}
                value={answers.adminAddresses}
                placeholder="0x..."
                onChange={(e) => onChange("adminAddresses", e.target.value)}
              />
            </Field>
          </section>

          <section className="srvsetup-section">
            <h2 className="srvsetup-h2">Content sync</h2>
            <Field
              label="Upstream peers"
              hint="Content servers this node mirrors. Empty uses the module's default peer set."
              errors={issuesFor(issues, "syncSources")}
            >
              <textarea
                className="srvsetup-input srvsetup-mono"
                rows={2}
                value={answers.syncSources}
                placeholder="https://peer.example.net/content"
                onChange={(e) => onChange("syncSources", e.target.value)}
              />
            </Field>
          </section>

          {publicShape ? (
            <section className="srvsetup-section">
              <h2 className="srvsetup-h2">Comms & extras</h2>
              <Field
                label="LiveKit advertised IP"
                hint="Single public IP clients dial for voice. Empty advertises every interface -- fine on a simple host, wrong behind NAT. API keys generate themselves."
                errors={issuesFor(issues, "livekitNodeIp")}
              >
                <TextInput
                  answers={answers}
                  field="livekitNodeIp"
                  onChange={onChange}
                  placeholder="203.0.113.7"
                />
              </Field>
              <label className="srvsetup-toggle">
                <input
                  type="checkbox"
                  checked={answers.playEnabled}
                  onChange={(e) => onChange("playEnabled", e.target.checked)}
                />
                <span>
                  Serve the in-browser client at /play (needs a bevy-explorer input in your
                  flake)
                </span>
              </label>
              <label className="srvsetup-toggle">
                <input
                  type="checkbox"
                  checked={answers.federationSeed}
                  onChange={(e) => onChange("federationSeed", e.target.checked)}
                />
                <span>
                  Ship the federation peers template &#x2014; its root certificates start blank,
                  and the worlds server refuses a blank-rooted file, so /worlds stays off
                  until you fill them. Untick to serve worlds non-federated right away.
                </span>
              </label>
            </section>
          ) : null}
        </div>

        <div className="srvsetup-result">
          <section className="srvsetup-section">
            <h2 className="srvsetup-h2">
              {ready ? "Your configuration" : "Configuration preview"}
            </h2>
            {!ready ? (
              <p className="srvsetup-note" role="status">
                {issues.length > 0
                  ? `${issues.length} answer${issues.length === 1 ? " needs" : "s need"} fixing before this is complete.`
                  : "Fill in the domain to complete the configuration."}
              </p>
            ) : null}
            <div className="srvsetup-file">
              <p className="srvsetup-filepath srvsetup-mono">{output.hostNix.path}</p>
              <pre className="srvsetup-pre">{output.hostNix.body}</pre>
            </div>
            {output.secrets.map((s) => (
              <div key={s.path} className="srvsetup-file">
                <p className="srvsetup-filepath srvsetup-mono">{s.path}</p>
                <pre className="srvsetup-pre">{s.body}</pre>
              </div>
            ))}
          </section>
          <section className="srvsetup-section">
            <h2 className="srvsetup-h2">Before first boot</h2>
            <ol className="srvsetup-checklist">
              {output.checklist.map((c) => (
                <li key={c}>{c}</li>
              ))}
            </ol>
          </section>
        </div>
      </div>
    </main>
  );
}
