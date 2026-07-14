import { renderToString } from "react-dom/server";
import { describe, expect, it } from "vitest";

import ServerOpsPage from "@ui/admin/pages/ServerOpsPage";
import type { ServerServiceRow } from "@ui/admin/pages/ServerOpsTypes";

const upService: ServerServiceRow = {
  key: "content",
  name: "Content + Lambdas",
  unit: "catalyrst-sync",
  port: 5141,
  url: "http://127.0.0.1:5141/about",
  serves: "catalyst content/lambdas API",
  state: "ok",
  httpStatus: 200,
  latencyMs: 12,
  detail: "HTTP 200 in 12ms",
  actionables: [],
  ageMs: 0,
};

const downService: ServerServiceRow = {
  ...upService,
  key: "livekit",
  name: "LiveKit SFU",
  unit: "livekit",
  port: 7880,
  url: "http://127.0.0.1:7880/",
  serves: "voice + comms media transport",
  state: "down",
  httpStatus: 0,
  latencyMs: 1500,
  detail: "connection refused \u{2014} nothing is listening",
  actionables: ["$ systemctl status livekit"],
  ageMs: 42_000,
};

const offService: ServerServiceRow = {
  ...upService,
  key: "governance",
  name: "Governance",
  unit: "catalyrst-governance",
  port: 5151,
  url: "http://127.0.0.1:5151/health",
  serves: "governance API (proposals, votes)",
  state: "off",
  httpStatus: 0,
  latencyMs: 0,
  detail: "not enabled by this node's configuration",
  actionables: [],
  ageMs: 0,
};

function textOf(html: string): string {
  return html.replace(/<!-- -->/g, "");
}

describe("ServerOpsPage SSR", () => {
  it("narrates unhealthy state: scoped recheck, watch cadence, ages, commands", () => {
    const html = textOf(
      renderToString(
        <ServerOpsPage
          services={[upService, downService, offService]}
          env={{
            ok: false,
            message: "Operator env persistence is not wired.",
            fix: "set CATALYRST_OPERATOR_ENV_FILE",
          }}
          authMode="edge"
          watch={{ intervalMs: 5000, checking: false }}
          recheckingKey="livekit"
          disk={{ path: "/", totalBytes: 100e9, freeBytes: 40e9, usedPercent: 60 }}
        />,
      ),
    );
    expect(html).toContain("Recheck all");
    expect(html).toContain("1 of 2 up \u{B7} 1 down \u{B7} rechecking every 5s until they recover");
    expect(html).toContain("Rechecking\u{2026}");
    expect(html).toContain("connection refused");
    expect(html).toContain("checked 42s ago");
    expect(html).toContain("systemctl status livekit");
    expect(html).toContain("1 service not enabled on this node");
    expect(html).toContain("Disk (/): 60% used \u{B7} 40.0 GB free");
    expect(html).not.toContain("full disk takes PostgreSQL");
    expect(html).toContain("Fix: set CATALYRST_OPERATOR_ENV_FILE");
    expect(html).toContain("ADMIN_WALLETS");
  });

  it("keeps healthy rows quiet: no commands, no chips, no stranded labels", () => {
    const html = textOf(
      renderToString(
        <ServerOpsPage
          services={[upService]}
          env={{
            ok: true,
            data: {
              path: "/var/lib/catalyrst-sites/operator.env",
              preservedLines: 0,
              rows: [
                {
                  name: "CATALYST_URL",
                  purpose: "Base URL SSR uses for catalyst reads.",
                  secret: false,
                  fileValue: "http://127.0.0.1:5141",
                  liveValue: "http://127.0.0.1:5141",
                  liveInSites: true,
                  pendingRestart: false,
                },
              ],
            },
          }}
          authMode="wallet"
        />,
      ),
    );
    expect(html).toContain("1 of 1 up");
    expect(html).not.toContain("systemctl");
    expect(html).not.toContain("srvops-remedy");
    expect(html).not.toContain("restart to apply");
    expect(html).not.toContain("set outside this file");
    expect(html).not.toContain("not enabled on this node");
    expect(html).not.toContain("hand-written");
  });

  it("chips carry only actionable env state and secrets never render", () => {
    const html = textOf(
      renderToString(
        <ServerOpsPage
          services={[upService]}
          env={{
            ok: true,
            data: {
              path: "/var/lib/catalyrst-sites/operator.env",
              preservedLines: 2,
              rows: [
                {
                  name: "SOME_API_TOKEN",
                  secret: true,
                  fileValue: "",
                  liveValue: null,
                  liveInSites: false,
                  pendingRestart: true,
                },
                {
                  name: "WORLDS_URL",
                  purpose: "Worlds content server base URL.",
                  secret: false,
                  fileValue: null,
                  liveValue: "http://127.0.0.1:5143",
                  liveInSites: true,
                  pendingRestart: false,
                },
              ],
            },
          }}
          authMode="wallet"
          notice={{ ok: true, message: "SOME_API_TOKEN saved." }}
          pendingEnv={{ name: "SOME_API_TOKEN", intent: "env-save" }}
        />,
      ),
    );
    expect(html).toContain("saved \u{2014} restart to apply");
    expect(html).toContain("set outside this file");
    expect(html).toContain('type="password"');
    expect(html).toContain("Saving\u{2026}");
    expect(html).toContain("SOME_API_TOKEN saved.");
    expect(html).toContain("http://127.0.0.1:5143");
    expect(html).toContain("2 hand-written lines in the file are kept as-is");
    expect(html).toContain("NEW_VARIABLE");
  });

  it("marks a watched service that came back", () => {
    const html = textOf(
      renderToString(
        <ServerOpsPage
          services={[{ ...upService, recovered: true }]}
          env={{ ok: false, message: "x" }}
          authMode="wallet"
        />,
      ),
    );
    expect(html).toContain("recovered");
  });
});
