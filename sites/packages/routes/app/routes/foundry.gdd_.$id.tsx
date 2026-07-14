import { useEffect, useRef } from "react";
import { data } from "react-router";

import EmptyState from "@ui/components/EmptyState";
import FdGddDocPage from "@ui/foundry/pages/FdGddDocPage";

import "@ui/components/emptystate.css";
import "@ui/foundry/components/fdprovenancepill.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdtable.css";
import "@ui/foundry/pages/fdgdd.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { FoundryUnavailableError, getPool, sidBadge } from "@data/lib/foundry/db.server";
import { getGddDoc } from "@data/lib/foundry/gdd.server";
import type { GddDoc } from "@data/lib/foundry/types";

import type { Route } from "./+types/foundry.gdd_.$id";

const LIST_HREF = "/foundry/gdd";

export function meta({ loaderData }: Route.MetaArgs) {
  const title = loaderData?.doc?.title;
  return [{ title: title ? `${title} — The Foundry` : "Design doc — The Foundry" }];
}

export async function loader({ params, request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);

  let doc: GddDoc | null = null;
  let unavailable = false;

  try {
    doc = await getGddDoc(getPool(), params.id);
    if (!doc) throw data(null, { status: 404 });
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    unavailable = true;
  }

  return wrap({ badge: sidBadge(sid), unavailable, doc });
}

export default function FoundryGddDoc({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const ctx = { sid: d.badge };
  const doc = d.doc;

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current || !doc) return;
    viewed.current = true;
    track(
      "fd_gdd_viewed",
      {
        doc_id: doc.id,
        kind: doc.kind,
        open_sections: doc.honesty.sections.filter((s) => s.open > 0).length,
        hypotheses: doc.hypotheses.length,
      },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge, doc?.id]);

  if (d.unavailable || !doc) {
    return (
      <EmptyState
        variant="inline"
        title="Foundry database not configured"
        subtitle="This page reads one design document out of Postgres. With no database behind it there is nothing to read."
        actions={[{ label: "All design docs", href: LIST_HREF, variant: "outline" }]}
      />
    );
  }

  return <FdGddDocPage doc={doc} backHref={LIST_HREF} />;
}
