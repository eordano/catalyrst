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
  it("renders the default public-gateway shape with the required-input errors named", () => {
    const html = page(WIZARD_DEFAULTS);
    expect(html).toContain("Set up this server");
    expect(html).toContain("Public gateway");
    expect(html).toContain("a host name is required");
    expect(html).toContain("archive-capable Ethereum JSON-RPC");
    expect(html).toContain("Configuration preview");
    expect(html).toContain('profile = "public-gateway";');
    expect(html).toContain("Before first boot");
  });

  it("renders a complete configuration with its secrets and DNS checklist", () => {
    const html = page({
      ...WIZARD_DEFAULTS,
      domain: "realm.example.org",
      acmeEmail: "ops@example.org",
      squidEthRpc: "https://eth.example.org",
      squidPolygonRpc: "https://polygon.example.org",
      sqdPortalKey: "sqd_testkey",
      adminAddresses: "0x1111111111111111111111111111111111111111",
    });
    expect(html).toContain("Your configuration");
    expect(html).toContain("catalyrst-host.nix");
    expect(html).toContain("/var/lib/secrets/squid.env");
    expect(html).toContain("*.realm.example.org");
    expect(html).not.toContain("needs fixing");
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
