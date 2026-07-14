import {
  buildEphemeralMessage,
  DEFAULT_EXPIRATION_MS,
  generateEphemeralKey,
} from "./identity";
import { loginWithIdentity } from "./engineLogin";
import type { AuthIdentity, AuthLink } from "./identity";

export const PAIR_API_PATH = "/internal/pair";
const POLL_INTERVAL_MS = 1_500;

export type WalletDeepLink = { name: string; href: string };

export type PhonePairSession = {
  uri: string;
  qrDataUrl: string;
  libreUri: string;
  deepLinks: WalletDeepLink[];
  connected: Promise<AuthIdentity | null>;
  cancel: () => void;
};

function buildLibreUri(id: string, relayOrigin: string): string {
  return `libre:${id}@1?relay=${encodeURIComponent(relayOrigin)}`;
}

function walletDeepLinks(pageUrl: string): WalletDeepLink[] {
  const encoded = encodeURIComponent(pageUrl);
  const schemeless = pageUrl.replace(/^https?:\/\//, "");
  let origin = "";
  try {
    origin = new URL(pageUrl).origin;
  } catch {
    /* fall back to empty Phantom ref */
  }
  // MetaMask: metamask.app.link, not link.metamask.io -- the current-docs
  // domain opens the wallet but not its in-app browser on many builds
  // (metamask-mobile #4760-class bugs); the legacy universal link routes
  // correctly. None of these need LibreConnect/WalletConnect support: each
  // just opens the https pair page in the wallet's dapp browser, where the
  // injected provider signs.
  return [
    { name: "MetaMask", href: `https://metamask.app.link/dapp/${schemeless}` },
    { name: "Coinbase Wallet", href: `https://go.cb-w.com/dapp?cb_url=${encoded}` },
    {
      name: "Trust Wallet",
      href: `https://link.trustwallet.com/open_url?coin_id=60&url=${encoded}`,
    },
    {
      name: "Phantom",
      href: `https://phantom.app/ul/browse/${encoded}?ref=${encodeURIComponent(origin)}`,
    },
    {
      name: "OKX Wallet",
      href: `https://web3.okx.com/download?deeplink=${encodeURIComponent(
        `okx://wallet/dapp/url?dappUrl=${encoded}`,
      )}`,
    },
  ];
}

async function qrSvgDataUrl(text: string): Promise<string> {
  const { default: qrcode } = await import("qrcode-generator");
  const qr = qrcode(0, "M");
  qr.addData(text);
  qr.make();
  const svg = qr.createSvgTag({ cellSize: 6, margin: 2, scalable: true });
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

export async function startPhonePairing(
  opts: { expirationMs?: number } = {},
): Promise<PhonePairSession> {
  const ephemeral = generateEphemeralKey();
  const expiration = new Date(
    Date.now() + (opts.expirationMs ?? DEFAULT_EXPIRATION_MS),
  );
  const expirationIso = expiration.toISOString();
  const message = buildEphemeralMessage(ephemeral.address, expiration);

  const created = await fetch(PAIR_API_PATH, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      kind: "create",
      ephemeral: ephemeral.address,
      expiration: expirationIso,
    }),
  });
  if (!created.ok) {
    throw new Error(
      created.status === 429
        ? "Too many sign-in attempts \u{2014} wait a minute and try again."
        : "Couldn't start phone sign-in.",
    );
  }
  const { id, pollToken, expiresAt } = (await created.json()) as {
    id: string;
    pollToken: string;
    expiresAt: string;
  };
  const uri = `${window.location.origin}/auth/pair/${id}`;

  let cancelled = false;
  let timer: ReturnType<typeof setTimeout> | null = null;

  const connected = (async (): Promise<AuthIdentity | null> => {
    const deadline = Date.parse(expiresAt) || Date.now() + 5 * 60_000;
    while (!cancelled && Date.now() < deadline) {
      await new Promise<void>((resolve) => {
        timer = setTimeout(resolve, POLL_INTERVAL_MS);
      });
      if (cancelled) break;
      let res: Response;
      try {
        res = await fetch(PAIR_API_PATH, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ kind: "poll", id, pollToken }),
        });
      } catch {
        continue;
      }
      if (res.status === 404 || res.status === 410 || res.status === 403) break;
      if (!res.ok) continue;
      const body = (await res.json()) as {
        state?: string;
        signer?: string;
        signature?: string;
      };
      if (body.state !== "completed" || !body.signer || !body.signature) {
        continue;
      }

      const { recoverMessageAddress } = await import("viem");
      const recovered = await recoverMessageAddress({
        message,
        signature: body.signature as `0x${string}`,
      });
      if (recovered.toLowerCase() !== body.signer.toLowerCase()) break;

      const signer = body.signer.toLowerCase();
      const authChain: AuthLink[] = [
        { type: "SIGNER", payload: signer, signature: "" },
        { type: "ECDSA_EPHEMERAL", payload: message, signature: body.signature },
      ];
      const next: AuthIdentity = {
        signer,
        ephemeral,
        expiration: expirationIso,
        authChain,
      };
      loginWithIdentity(next);
      return next;
    }
    return null;
  })();

  return {
    uri,
    qrDataUrl: await qrSvgDataUrl(uri),
    libreUri: buildLibreUri(id, window.location.origin),
    deepLinks: walletDeepLinks(uri),
    connected,
    cancel: () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
      void fetch(PAIR_API_PATH, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ kind: "cancel", id, pollToken }),
      }).catch(() => undefined);
    },
  };
}
