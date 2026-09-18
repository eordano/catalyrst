import { renderToString } from "react-dom/server";
import { describe, expect, it } from "vitest";

import ServerSetupPage from "@ui/admin/pages/ServerSetupPage";

import {
  WIZARD_DEFAULTS,
  type WizardAnswers,
  generateWizardOutput,
  wizardIssues,
} from "@data/lib/operator/wizard";

function page(answers: WizardAnswers): string {
  return renderToString(
    <ServerSetupPage
      answers={answers}
      issues={wizardIssues(answers)}
      output={generateWizardOutput(answers)}
      onChange={() => {}}
      serverHref="/server"
    />,
  )
    .replace(/<!-- -->/g, "")
    .replace(/&quot;/g, '"')
    .replace(/&#x27;/g, "'")
    .replace(/&amp;/g, "&");
}

describe("ServerSetupPage SSR", () => {
  it("names the required inputs on the default public-gateway shape and renders a complete configuration with its secrets and DNS checklist", () => {
    const defaults = page(WIZARD_DEFAULTS);
    expect(defaults).toContain("Set up this server");
    expect(defaults).toContain("Public gateway");
    expect(defaults).toContain("a host name is required");
    expect(defaults).toContain("archive-capable Ethereum JSON-RPC");
    expect(defaults).toContain("Configuration preview");
    expect(defaults).toContain('profile = "public-gateway";');
    expect(defaults).toContain("Before first boot");

    const complete = page({
      ...WIZARD_DEFAULTS,
      domain: "realm.example.org",
      acmeEmail: "ops@example.org",
      squidEthRpc: "https://eth.example.org",
      squidPolygonRpc: "https://polygon.example.org",
      sqdPortalKey: "sqd_testkey",
      adminAddresses: "0x1111111111111111111111111111111111111111",
    });
    expect(complete).toContain("Your configuration");
    expect(complete).toContain("catalyrst-host.nix");
    expect(complete).toContain("/var/lib/secrets/squid.env");
    expect(complete).toContain("*.realm.example.org");
    expect(complete).not.toContain("needs fixing");
  });

  it("hides public-only sections for a content node and never names a foreign host", () => {
    const html = page({
      ...WIZARD_DEFAULTS,
      profile: "content-node",
      domain: "node.home.arpa",
      tls: "none",
    });
    expect(html).not.toContain("LiveKit advertised IP");
    expect(html).not.toContain("archive RPC");
    expect(html).toContain('profile = "content-node";');
    expect(html).not.toMatch(/dcl\.one|decentraland\.org|interconnected/i);
  });
});
