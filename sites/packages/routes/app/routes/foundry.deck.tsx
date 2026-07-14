import { useEffect, useRef } from "react";

import FdDeckPage from "@ui/foundry/pages/FdDeckPage";

import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdtable.css";
import "@ui/foundry/pages/fddeck.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { sidBadge } from "@data/lib/foundry/db.server";

import type { Route } from "./+types/foundry.deck";

export function meta() {
  return [{ title: "The deck — The Foundry" }];
}

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);
  return wrap({ badge: sidBadge(sid) });
}

export default function FoundryDeck({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track("fd_deck_viewed", {}, { sid: d.badge });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge]);

  return <FdDeckPage />;
}
