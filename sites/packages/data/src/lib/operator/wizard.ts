/**
 * First-boot wizard logic for /server/setup: pure functions from operator
 * answers to a ready-to-paste NixOS host module, the secrets files the units
 * fail-closed without, and a first-boot checklist. Everything emitted is
 * domain-neutral -- the operator's own domain is the only host name that ever
 * appears. Shapes mirror nixos/module-example.nix (the consumer authority)
 * and docs/self-host.md (the secrets table).
 */

export type WizardProfile = "content-node" | "full-realm" | "public-gateway";
export type WizardTls = "acme-http01" | "acme-dns01" | "none";

export type WizardAnswers = {
  profile: WizardProfile;
  domain: string;
  tls: WizardTls;
  acmeEmail: string;
  adminAddresses: string;
  ethRpcUrl: string;
  squidEthRpc: string;
  squidPolygonRpc: string;
  sqdPortalKey: string;
  syncSources: string;
  livekitNodeIp: string;
  playEnabled: boolean;
  federationSeed: boolean;
};

export const WIZARD_DEFAULTS: WizardAnswers = {
  profile: "public-gateway",
  domain: "",
  tls: "acme-dns01",
  acmeEmail: "",
  adminAddresses: "",
  ethRpcUrl: "",
  squidEthRpc: "",
  squidPolygonRpc: "",
  sqdPortalKey: "",
  syncSources: "",
  livekitNodeIp: "",
  playEnabled: true,
  federationSeed: true,
};

export type WizardIssue = { field: keyof WizardAnswers; message: string };

export type WizardFile = { path: string; body: string };

export type WizardOutput = {
  hostNix: WizardFile;
  secrets: WizardFile[];
  checklist: string[];
};

const SECRETS_DIR = "/var/lib/secrets";

/**
 * Subdomains an acme-http01 public-gateway certificate must cover, mirrored
 * from the vhost gates in nixos/web.nix + nixos/web-gateway.nix so the DNS
 * checklist is concrete. Treat a disagreement with those files as a bug here.
 */
const BASE_SUBDOMAINS = ["www", "abgen", "livekit"];
const GATEWAY_SUBDOMAINS = [
  "gateway",
  "peer",
  "asset-bundle-registry",
  "opensea",
  "realm-provider-ea",
  "auth-api",
  "places",
  "api",
  "archipelago-ea-stats",
  "badges",
  "notifications",
  "assets-cdn",
  "metamorph-api",
  "camera-reel-service",
  "credits",
  "ab-cdn",
  "profile-images",
  "feature-flags",
  "config",
  "dcl-lists",
  "events",
  "worlds-content-server",
  "comms-gatekeeper",
  "social-api",
  "rpc",
  "rpc-social-service-ea",
];

const WALLET_RE = /^0x[0-9a-fA-F]{40}$/;
const IPV4_RE = /^(\d{1,3}\.){3}\d{1,3}$/;

export function parseWallets(raw: string): string[] {
  return raw
    .split(/[\s,]+/)
    .map((w) => w.trim())
    .filter(Boolean);
}

export function parseSyncSources(raw: string): string[] {
  return raw
    .split(/[\s,]+/)
    .map((s) => s.trim())
    .filter(Boolean);
}

function isHttpUrl(v: string): boolean {
  try {
    const u = new URL(v);
    return u.protocol === "http:" || u.protocol === "https:";
  } catch {
    return false;
  }
}

export function isPublicProfile(profile: WizardProfile): boolean {
  return profile !== "content-node";
}

export function wizardIssues(a: WizardAnswers): WizardIssue[] {
  const issues: WizardIssue[] = [];
  const publicShape = isPublicProfile(a.profile);

  if (a.domain.trim() === "") {
    issues.push({ field: "domain", message: "a host name is required" });
  } else if (publicShape && !a.domain.includes(".")) {
    issues.push({
      field: "domain",
      message: "public profiles need a fully qualified domain the internet can resolve",
    });
  }

  if (publicShape && a.tls !== "acme-http01" && a.tls !== "acme-dns01") {
    issues.push({
      field: "tls",
      message:
        "public profiles need an ACME mode \u{2014} the public vhosts consume a real certificate",
    });
  }
  if ((a.tls === "acme-http01" || a.tls === "acme-dns01") && !/^\S+@\S+\.\S+$/.test(a.acmeEmail)) {
    issues.push({
      field: "acmeEmail",
      message: "ACME issuance requires a contact email (expiry notices go there)",
    });
  }

  for (const w of parseWallets(a.adminAddresses)) {
    if (!WALLET_RE.test(w)) {
      issues.push({ field: "adminAddresses", message: `${w} is not a 0x wallet address` });
    }
  }

  if (a.ethRpcUrl.trim() !== "" && !isHttpUrl(a.ethRpcUrl.trim())) {
    issues.push({ field: "ethRpcUrl", message: "must be an http(s) URL" });
  }

  if (a.profile === "public-gateway") {
    if (!isHttpUrl(a.squidEthRpc.trim())) {
      issues.push({
        field: "squidEthRpc",
        message:
          "the marketplace indexer needs an archive-capable Ethereum JSON-RPC endpoint to start",
      });
    }
    if (!isHttpUrl(a.squidPolygonRpc.trim())) {
      issues.push({
        field: "squidPolygonRpc",
        message:
          "the marketplace indexer needs an archive-capable Polygon JSON-RPC endpoint to start",
      });
    }
    if (a.sqdPortalKey.trim() === "") {
      issues.push({
        field: "sqdPortalKey",
        message:
          "the polygon processor only runs against the authenticated SQD portal \u{2014} free key at sqd.ai",
      });
    }
  }

  for (const s of parseSyncSources(a.syncSources)) {
    if (!isHttpUrl(s)) {
      issues.push({ field: "syncSources", message: `${s} is not an http(s) URL` });
    }
  }

  if (a.livekitNodeIp.trim() !== "" && !IPV4_RE.test(a.livekitNodeIp.trim())) {
    issues.push({ field: "livekitNodeIp", message: "must be a single IPv4 address" });
  }

  return issues;
}

function nixStr(v: string): string {
  return `"${v.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
}

function hostNixBody(a: WizardAnswers): string {
  const acme = a.tls === "acme-http01" || a.tls === "acme-dns01";
  const wallets = parseWallets(a.adminAddresses);
  const sources = parseSyncSources(a.syncSources);
  const lines: string[] = [];

  lines.push("{ inputs, pkgs, ... }:");
  lines.push("{");
  lines.push("  imports = [ inputs.catalyrst.nixosModules.catalyrst ];");
  lines.push("");
  if (acme) {
    lines.push(`  security.acme.defaults.email = ${nixStr(a.acmeEmail.trim())};`);
    lines.push("");
  }
  lines.push("  services.catalyrst = {");
  lines.push("    enable = true;");
  lines.push(`    profile = ${nixStr(a.profile)};`);
  lines.push(`    domain = ${nixStr(a.domain.trim())};`);
  lines.push(`    tls = ${nixStr(a.tls)};`);
  if (wallets.length > 0) {
    lines.push("    adminAddresses = [");
    for (const w of wallets) lines.push(`      ${nixStr(w)}`);
    lines.push("    ];");
  }
  if (a.ethRpcUrl.trim() !== "") {
    lines.push(`    ethRpcUrl = ${nixStr(a.ethRpcUrl.trim())};`);
  }
  if (sources.length > 0) {
    lines.push("    sync.sources = [");
    for (const s of sources) lines.push(`      ${nixStr(s)}`);
    lines.push("    ];");
  }
  if (isPublicProfile(a.profile) && a.livekitNodeIp.trim() !== "") {
    lines.push(`    livekit.nodeIp = ${nixStr(a.livekitNodeIp.trim())};`);
  }
  if (isPublicProfile(a.profile) && a.playEnabled) {
    lines.push("");
    lines.push("    play = {");
    lines.push("      enable = true;");
    lines.push("      package = inputs.bevy-explorer.packages.${pkgs.system}.web;");
    lines.push("    };");
  }
  if (!a.federationSeed) {
    lines.push("");
    lines.push("    federation.seedDefault = false;");
  }
  lines.push("  };");
  lines.push("}");
  return lines.join("\n") + "\n";
}

function squidEnvBody(a: WizardAnswers): string {
  return (
    [
      `RPC_ENDPOINT_ETH=${a.squidEthRpc.trim()}`,
      `RPC_ENDPOINT_POLYGON=${a.squidPolygonRpc.trim()}`,
      `SQD_PORTAL_API_KEY=${a.sqdPortalKey.trim()}`,
      "DB_SCHEMA=squid_marketplace",
      "DB_HOST=/run/postgresql",
      "DB_NAME=marketplace_squid",
      "DB_USER=squid",
      "",
      "# Optional portal override:",
      "# SQD_PORTAL_URL=",
    ].join("\n") + "\n"
  );
}

function checklist(a: WizardAnswers): string[] {
  const d = a.domain.trim() || "<domain>";
  const items: string[] = [];
  const publicShape = isPublicProfile(a.profile);

  if (!publicShape) {
    items.push(`DNS: point ${d} at this host (a LAN name or IP works; the edge serves plain HTTP on port 80).`);
    items.push("Firewall: allow TCP 80 from your network.");
  } else {
    const subs = [
      ...BASE_SUBDOMAINS,
      ...(a.profile === "public-gateway" ? GATEWAY_SUBDOMAINS : []),
    ];
    if (a.tls === "acme-dns01") {
      items.push(
        `DNS: an A record for ${d} plus a wildcard *.${d}, both pointing at this host.`,
      );
      items.push(
        `Secrets: ${SECRETS_DIR}/cloudflare-dns.env with the DNS-API credentials the wildcard certificate is issued through.`,
      );
    } else {
      items.push(
        `DNS: A records (or CNAMEs) for ${d} and every certificate name, all pointing at this host \u{2014} each one is challenged over HTTP-01 at issuance: ${subs
          .map((s) => `${s}.${d}`)
          .join(", ")}.`,
      );
    }
    items.push(
      "Firewall: allow TCP 80 + 443 (edge), TCP 7881 + UDP 7882 (LiveKit media), UDP 7777 (pulse comms).",
    );
    if (a.livekitNodeIp.trim() === "") {
      items.push(
        "LiveKit will advertise every interface it finds; on a multi-homed or NATed host, set the single public IP in this wizard instead.",
      );
    }
  }

  if (a.profile === "public-gateway") {
    items.push(
      `Secrets: install squid.env (below) at ${SECRETS_DIR}/squid.env before first boot \u{2014} the indexer units fail closed without it.`,
    );
  }
  items.push(
    "Session secrets, LiveKit API keys and the storage encryption key generate themselves on first boot \u{2014} nothing to supply.",
  );
  if (parseWallets(a.adminAddresses).length > 0) {
    items.push(
      "Wallet auth: adminAddresses in catalyrst-host.nix gates /admin and /server \u{2014} sign in with one of those wallets.",
    );
  } else {
    items.push(
      "No admin wallets configured: /server stays reachable only through the edge allowlist (loopback by default). Add wallets any time via this wizard or the /server environment editor.",
    );
  }
  items.push(
    `First boot: nixos-rebuild switch, then check from outside \u{2014} /about on ${d} answers with the node's manifest as soon as the content server is up. The /server operations page shows per-service health (public-gateway serves it; the edge allowlist and admin wallets gate it), and systemctl status / journalctl on the node have the detail.`,
  );
  if (a.federationSeed) {
    items.push(
      "Federation: the shipped peers file carries blank root certificates, and the worlds server refuses to load it that way \u{2014} the /worlds surface stays absent until you fill each peer's mtls_root_pem in it. Fill the roots, or untick the template to serve worlds non-federated.",
    );
  } else {
    items.push(
      "Worlds serves non-federated. To federate later, provide a completed peers file (real root certificates) and point federation at it.",
    );
  }
  return items;
}

export function generateWizardOutput(a: WizardAnswers): WizardOutput {
  const secrets: WizardFile[] = [];
  if (a.profile === "public-gateway") {
    secrets.push({ path: `${SECRETS_DIR}/squid.env`, body: squidEnvBody(a) });
  }
  return {
    hostNix: { path: "catalyrst-host.nix", body: hostNixBody(a) },
    secrets,
    checklist: checklist(a),
  };
}
