import { setIdentity } from "./session";
import type { AuthIdentity, AuthLink } from "./types";

export const PAIR_API_PATH = "/internal/pair";
const POLL_INTERVAL_MS = 1_500;

export type PhonePairSession = {
  uri: string;
  qrDataUrl: string;
  connected: Promise<AuthIdentity | null>;
  cancel: () => void;
};

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
  const { buildEphemeralMessage, DEFAULT_EXPIRATION_MS, generateEphemeralKey } =
    await import("./identity");
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
      setIdentity(next);
      return next;
    }
    return null;
  })();

  return {
    uri,
    qrDataUrl: await qrSvgDataUrl(uri),
    connected,
    cancel: () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
      void fetch(PAIR_API_PATH, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ kind: "cancel", id, pollToken }),
      }).catch(() => {});
    },
  };
}
