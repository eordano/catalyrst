import { useEffect, useRef } from "react";

import EmptyState from "@ui/components/EmptyState";
import FdGddListPage from "@ui/foundry/pages/FdGddListPage";

import "@ui/components/emptystate.css";
import "@ui/foundry/components/fdprovenancepill.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/pages/fdgdd.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { FoundryUnavailableError, getPool, sidBadge } from "@data/lib/foundry/db.server";
import { listGddDocs, type GddListRow } from "@data/lib/foundry/gdd.server";

import type { Route } from "./+types/foundry.gdd";

export function meta() {
  return [{ title: "Design docs — The Foundry" }];
}

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);

  let docs: GddListRow[] = [];
  let unavailable = false;

  try {
    docs = await listGddDocs(getPool());
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    unavailable = true;
  }

  return wrap({ badge: sidBadge(sid), unavailable, docs });
}

export default function FoundryGddList({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const ctx = { sid: d.badge };

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track("fd_gdd_list_viewed", { docs: d.docs.length }, ctx);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge]);

  if (d.unavailable) {
    return (
      <EmptyState
        variant="inline"
        title="Foundry database not configured"
        subtitle="Design documents are stored in Postgres. Set FOUNDRY_DATABASE_URL on this deployment to read them."
      />
    );
  }

  return <FdGddListPage docs={d.docs} />;
}
