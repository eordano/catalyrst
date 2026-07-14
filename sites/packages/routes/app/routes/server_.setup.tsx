import { useState } from "react";

import ServerSetupPage from "@ui/admin/pages/ServerSetupPage";
import SitesChrome from "@ui/web/frames/SitesChrome";

import { readVerifiedWallet } from "@core/lib/experiments/assign";
import {
  WIZARD_DEFAULTS,
  type WizardAnswers,
  generateWizardOutput,
  wizardIssues,
} from "@data/lib/operator/wizard";

import type { Route } from "./+types/server_.setup";

export function meta() {
  return [{ title: "Server setup" }];
}

/**
 * Same gate as /server: ADMIN_WALLETS unset trusts the edge allowlist (the
 * module routes /server/* through the nginx superadmin gate), set requires a
 * verified operator wallet. The wizard itself is pure client-side generation
 * -- no action, nothing persisted server-side.
 */
export async function loader({ request }: Route.LoaderArgs) {
  const raw = process.env.ADMIN_WALLETS;
  const wallets = (raw ?? "")
    .split(",")
    .map((w) => w.trim().toLowerCase())
    .filter(Boolean);
  if (wallets.length === 0) return { authorized: true as const };
  const wallet = readVerifiedWallet(request);
  if (wallet && wallets.includes(wallet.toLowerCase())) {
    return { authorized: true as const };
  }
  return {
    authorized: false as const,
    reason: wallet
      ? "This wallet is not in ADMIN_WALLETS."
      : "ADMIN_WALLETS is set, so this page needs a signed-in operator wallet.",
  };
}

export default function ServerSetupRoute({ loaderData }: Route.ComponentProps) {
  const [answers, setAnswers] = useState<WizardAnswers>(WIZARD_DEFAULTS);

  if (!loaderData.authorized) {
    return (
      <SitesChrome>
        <main style={{ maxWidth: 640, margin: "0 auto", padding: "48px 20px" }}>
          <h1>Server setup</h1>
          <p>{loaderData.reason}</p>
        </main>
      </SitesChrome>
    );
  }

  return (
    <SitesChrome>
      <ServerSetupPage
        answers={answers}
        issues={wizardIssues(answers)}
        output={generateWizardOutput(answers)}
        onChange={(field, value) => setAnswers((a) => ({ ...a, [field]: value }))}
        serverHref="/server"
      />
    </SitesChrome>
  );
}
