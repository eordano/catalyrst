
import { signInWithPrivateKey } from "@data/lib/auth/dev-identity";
import { getIdentity, setIdentity } from "@data/lib/auth/session";
import { clearThirdwebSession } from "@data/lib/auth/thirdweb/session";
import type { AuthIdentity } from "@data/lib/auth/types";

export type DclDevKit = {
  signInBurner: (pk?: `0x${string}`) => Promise<AuthIdentity>;
  seedIdentity: (address: string, opts?: { expirationMs?: number }) => AuthIdentity;
  signOut: () => void;
  identity: () => AuthIdentity | null;
  openSignIn: () => Promise<void>;
  closeSignIn: () => Promise<void>;
  preview: (component: string, props?: Record<string, unknown>, opts?: { wrap?: string }) => void;
  clearAll: () => void;
};

function makeKit(): DclDevKit {
  return {
    async signInBurner(pk) {
      const { generatePrivateKey } = await import("viem/accounts");
      const key = pk ?? generatePrivateKey();
      const identity = await signInWithPrivateKey(key, { allowNonDev: true });
      try {
        window.localStorage.setItem("dcl:auth:dev-signer-pk:v1", key);
      } catch {
      }
      return identity;
    },

    seedIdentity(address, opts = {}) {
      if (!/^0x[0-9a-fA-F]{40}$/.test(address)) {
        throw new Error("seedIdentity: address must be 0x + 40 hex");
      }
      const expiration = new Date(
        Date.now() + (opts.expirationMs ?? 24 * 60 * 60 * 1000),
      ).toISOString();
      const identity: AuthIdentity = {
        signer: address.toLowerCase(),
        ephemeral: {
          address: "0x" + "11".repeat(20),
          privateKey: ("0x" + "22".repeat(32)) as `0x${string}`,
        },
        expiration,
        authChain: [],
      };
      setIdentity(identity);
      return identity;
    },

    signOut() {
      setIdentity(null);
      clearThirdwebSession();
      try {
        window.localStorage.removeItem("dcl:auth:dev-signer-pk:v1");
      } catch {
      }
    },

    identity: () => getIdentity(),

    async openSignIn() {
      const { openSignIn } = await import("../../components/auth/signin-store");
      openSignIn();
    },

    async closeSignIn() {
      const { closeSignIn } = await import("../../components/auth/signin-store");
      closeSignIn();
    },

    preview(component, props, opts = {}) {
      const params = new URLSearchParams();
      if (props) params.set("props", JSON.stringify(props));
      if (opts.wrap) params.set("wrap", opts.wrap);
      const qs = params.toString();
      window.location.assign(
        `/dev/preview/${encodeURIComponent(component)}${qs ? `?${qs}` : ""}`,
      );
    },

    clearAll() {
      try {
        window.localStorage.clear();
        window.sessionStorage.clear();
      } catch {
      }
      window.location.reload();
    },
  };
}

export function installDevKit(): void {
  const w = window as unknown as { __DCL_DEV__?: DclDevKit };
  if (w.__DCL_DEV__) return;
  w.__DCL_DEV__ = makeKit();
  console.info(
    "[dcl-dev] window.__DCL_DEV__ ready:",
    Object.keys(w.__DCL_DEV__).join(", "),
  );
}
