import { useEffect } from "react";

import { getSharedSyncEngine } from "@ui/creatorhub/lib/shared-sync";

import { getIdentity } from "@data/lib/auth/session";
import { signRequest } from "@data/lib/auth/signer";

export default function SyncEngineBridge() {
  useEffect(() => {
    const engine = getSharedSyncEngine();

    engine.setTransport(async (id, { baseVersion, hash, blob, title }) => {
      const identity = getIdentity();
      if (!identity) throw new Error("sync: not signed in");

      const path = `/api/creator-hub/drafts/${id}`;
      const { headers } = await signRequest(identity, "PUT", path);

      const res = await fetch(path, {
        method: "PUT",
        headers: { ...headers, "content-type": "application/json" },
        body: JSON.stringify({ baseVersion, hash, title, blob }),
      });

      if (res.status === 409) {
        const s = (await res.json()) as { server?: { version?: number; blob?: unknown } };
        return { ok: false, conflict: true, version: s.server?.version, server: s.server?.blob };
      }
      if (!res.ok) throw new Error(`sync ${res.status}`);

      const s = (await res.json()) as { meta?: { version?: number } };
      return { ok: true, version: s.meta?.version };
    });

    engine.start();
    return () => engine.stop();
  }, []);

  return null;
}
