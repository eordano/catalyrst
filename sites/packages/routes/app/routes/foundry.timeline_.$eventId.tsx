import { data } from "react-router";

import FdTimelineEventPage, {
  type FdTimelineEventVM,
} from "@ui/foundry/pages/FdTimelineEventPage";
import { FD_UNAVAILABLE, FdPageHead } from "@ui/foundry/components/FdSection";

import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdpersonachip.css";
import "@ui/foundry/pages/fdtimelineevent.css";

import { sidLoader } from "@core/lib/experiments/story-loader";

import { FoundryUnavailableError, sidBadge } from "@data/lib/foundry/db.server";
import { getMemoryEvent } from "@data/lib/foundry/continuity.server";
import { entityHref } from "@data/lib/foundry/play.server";

import type { Route } from "./+types/foundry.timeline_.$eventId";

export function meta() {
  return [{ title: "Remembered event — The Foundry" }];
}

export async function loader({ params, request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);

  let event: FdTimelineEventVM | null = null;
  let unavailable = false;

  try {
    const record = await getMemoryEvent(params.eventId);
    if (!record) throw data("No remembered event with this id.", { status: 404 });
    event = {
      eventId: record.eventId,
      kind: record.kind,
      at: record.at,
      actor: record.actor,
      action: record.action,
      body: record.body,
      sourceNote: record.sourceNote,
      origin: record.origin,
      scene: {
        title: record.scene.title,
        href: `/foundry/play/${record.scene.id}`,
      },
      // Imported changelog rows derive from the deployment entity; visitor rows
      // derive from the action/changelog row itself and get no entity link.
      entityHref:
        record.origin === "import" ? entityHref(record.scene.entityId) : null,
    };
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    unavailable = true;
  }

  return wrap({ badge: sidBadge(sid), unavailable, event });
}

export default function FoundryTimelineEvent({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  if (d.unavailable || !d.event) {
    return (
      <div className="fd-page fd-stack fd-tlevent">
        <FdPageHead
          eyebrow="Timeline"
          title="Remembered event"
          crumbs={<a href="/foundry/timeline">← All events</a>}
        />
        <p className="fd-empty">{FD_UNAVAILABLE}</p>
      </div>
    );
  }

  return <FdTimelineEventPage event={d.event} />;
}
