import { describe, expect, it } from "vitest";

import {
  WIZARD_DEFAULTS,
  type WizardAnswers,
  generateWizardOutput,
  wizardIssues,
} from "./wizard";

function answers(overrides: Partial<WizardAnswers>): WizardAnswers {
  return { ...WIZARD_DEFAULTS, ...overrides };
}

const VALID_GATEWAY = answers({
  domain: "example.org",
  acmeEmail: "ops@example.org",
  squidEthRpc: "https://eth.example.org",
  squidPolygonRpc: "https://polygon.example.org",
  sqdPortalKey: "sqd_testkey",
  adminAddresses: "0x1111111111111111111111111111111111111111",
});

describe("wizardIssues", () => {
  it("passes a complete public gateway, requires squid inputs and an ACME mode on public profiles only, and rejects malformed wallets, RPC URLs and node IPs", () => {
    expect(wizardIssues(VALID_GATEWAY)).toEqual([]);

    const missing = wizardIssues(
      answers({ domain: "example.org", acmeEmail: "ops@example.org" }),
    );
    expect(missing.map((i) => i.field)).toEqual([
      "squidEthRpc",
      "squidPolygonRpc",
      "sqdPortalKey",
    ]);
    expect(
      wizardIssues(
        answers({ profile: "full-realm", domain: "example.org", acmeEmail: "ops@example.org" }),
      ),
    ).toEqual([]);

    expect(
      wizardIssues(answers({ ...VALID_GATEWAY, tls: "none" })).some((i) => i.field === "tls"),
    ).toBe(true);
    expect(
      wizardIssues(answers({ profile: "content-node", domain: "node.home.arpa", tls: "none" })),
    ).toEqual([]);

    const bad = wizardIssues({
      ...VALID_GATEWAY,
      adminAddresses: "0x123",
      ethRpcUrl: "not-a-url",
      livekitNodeIp: "not-an-ip",
    });
    expect(bad.map((i) => i.field).sort()).toEqual([
      "adminAddresses",
      "ethRpcUrl",
      "livekitNodeIp",
    ]);
  });
});

describe("generateWizardOutput", () => {
  it("emits the minimal complete public-gateway shape with wallet auth in the host module and squid.env carrying the peer-auth socket quintet", () => {
    const out = generateWizardOutput(VALID_GATEWAY);
    const nix = out.hostNix.body;
    expect(nix).toContain('profile = "public-gateway";');
    expect(nix).toContain('domain = "example.org";');
    expect(nix).toContain('security.acme.defaults.email = "ops@example.org";');
    expect(nix).toContain("inputs.bevy-explorer.packages.${pkgs.system}.web");
    expect(nix).toContain('"0x1111111111111111111111111111111111111111"');
    expect(nix).not.toContain("Package =");
    expect(nix).not.toContain("federation.seedDefault");
    expect(nix).not.toContain("ethRpcUrl");

    expect(out.secrets.some((s) => s.path.endsWith("sites.env"))).toBe(false);
    const squid = out.secrets.find((s) => s.path.endsWith("squid.env"));
    expect(squid?.body).toContain("RPC_ENDPOINT_ETH=https://eth.example.org");
    expect(squid?.body).toContain("RPC_ENDPOINT_POLYGON=https://polygon.example.org");
    expect(squid?.body).toContain("SQD_PORTAL_API_KEY=sqd_testkey");
    expect(squid?.body).toContain("DB_SCHEMA=squid_marketplace");
    expect(squid?.body).toContain("DB_HOST=/run/postgresql");
    expect(squid?.body).toContain("DB_USER=squid");
    expect(squid?.body).not.toContain("DB_PASS");
  });

  it("keeps a content node minimal", () => {
    const out = generateWizardOutput(
      answers({
        profile: "content-node",
        domain: "node.home.arpa",
        tls: "none",
        federationSeed: false,
        syncSources: "https://peer.example.net/content",
      }),
    );
    const nix = out.hostNix.body;
    expect(nix).toContain('profile = "content-node";');
    expect(nix).not.toContain("bundlesPackage");
    expect(nix).not.toContain("play");
    expect(nix).not.toContain("security.acme");
    expect(nix).toContain("federation.seedDefault = false;");
    expect(nix).toContain('"https://peer.example.net/content"');
    expect(out.secrets).toEqual([]);
  });

  it("the checklist names every http01 certificate (or the dns01 wildcard + token file) and the worlds consequence of the federation template in both states", () => {
    const http01 = generateWizardOutput({ ...VALID_GATEWAY, tls: "acme-http01" });
    const dns = http01.checklist.find((c) => c.startsWith("DNS:"));
    for (const sub of ["www", "abgen", "livekit", "gateway", "peer", "rpc-social-service-ea"]) {
      expect(dns).toContain(`${sub}.example.org`);
    }
    const dns01 = generateWizardOutput(VALID_GATEWAY);
    expect(dns01.checklist.find((c) => c.startsWith("DNS:"))).toContain("*.example.org");
    expect(dns01.checklist.some((c) => c.includes("cloudflare-dns.env"))).toBe(true);

    const shipped = dns01.checklist.join("\n");
    expect(shipped).toContain("/worlds surface stays absent");
    expect(shipped).toContain("mtls_root_pem");
    const unticked = generateWizardOutput({
      ...VALID_GATEWAY,
      federationSeed: false,
    }).checklist.join("\n");
    expect(unticked).toContain("Worlds serves non-federated");
    expect(unticked).not.toContain("stays absent");
  });

  it("never emits a foreign host name anywhere", () => {
    for (const shape of [
      VALID_GATEWAY,
      answers({ profile: "full-realm", domain: "my-node.net", acmeEmail: "a@b.co" }),
      answers({ profile: "content-node", domain: "10.0.0.5", tls: "none" }),
    ]) {
      const out = generateWizardOutput(shape);
      const all = [out.hostNix.body, ...out.secrets.map((s) => s.body), ...out.checklist].join(
        "\n",
      );
      expect(all).not.toMatch(/dcl\.one|decentraland\.org|interconnected/i);
    }
  });
});
