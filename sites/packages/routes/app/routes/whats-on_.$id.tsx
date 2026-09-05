import { useEffect, useRef } from "react";
import { Link } from "react-router";
import { href } from "@core/lib/router/routes";

import EventDetail from "@ui/explorer/pages/EventDetail";
import SitesChrome from "@ui/web/frames/SitesChrome";
import "@ui/explorer/pages/eventdetail.css";

import UpstreamUnavailable from "@features/components/UpstreamUnavailable";
import { CatalystError } from "@data/lib/catalyst/client";
import {
  fetchEvent,
  eventCoords,
  eventJumpUrl,
  formatEventWhen,
  type Event,
} from "@data/lib/catalyst/places/events";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/whats-on_.$id";
import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const handle = { agentMarkdown: "eventDetail" } satisfies AgentMarkdownHandle;

const STORY: StoryId = "admin/whats-on-detail";

const FALLBACK: Assignment = {
  variant: "detail_cta",
  flags: { prominentJumpIn: true },
  experimentKey: "lp_event_jump_in",
};

export async function loader({ request, params }: Route.LoaderArgs) {
  const { id } = params;
  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  let event: Event | null = null;
  let unavailable = false;
  try {
    event = await fetchEvent(id, { signal: request.signal });
  } catch (err) {
    if (!(err instanceof CatalystError && err.status === 404)) {
      unavailable = true;
    }
  }

  return wrap(
    { id, event, sid, unavailable },
    { status: event ? 200 : unavailable ? 503 : 404 },
  );
}

export default function EventDetailRoute({ loaderData }: Route.ComponentProps) {
  const { event, sid, unavailable } = loaderData;

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    if (event) {
      track(
        "lp_event_viewed",
        { event_id: event.id, live: event.live },
        { sid, story: STORY },
      );
    }
  }, [event, sid]);

  function onJumpIn() {
    if (!event) return;
    track(
      "lp_event_jump_in",
      { event_id: event.id, position: eventCoords(event) },
      { sid, story: STORY },
    );
  }

  return event ? (
    <main className="event-detail-route">
      <EventDetail
        {...({
          event: {
            title: event.name ?? "Untitled event",
            image: event.image ?? undefined,
            when: formatEventWhen(event.start_at ?? event.next_start_at),
            host: event.user_name ?? event.scene_name ?? "Decentraland",
            description: event.description ?? "No description provided.",
            schedule: formatEventWhen(event.start_at ?? event.next_start_at),
            location: eventCoords(event) ?? "Location not published",
          },
          jumpHref: eventJumpUrl(event) ?? event.url ?? undefined,
          onJumpIn,
        } as React.ComponentProps<typeof EventDetail>)}
      />
    </main>
  ) : unavailable ? (
    <SitesChrome active="whatson">
      <UpstreamUnavailable
        title="Events are temporarily unavailable"
        message="We couldn't reach the events service. Please try again in a moment."
        backHref={href("/whats-on")}
        backLabel="Back to What's On"
      />
    </SitesChrome>
  ) : (
    <SitesChrome active="whatson">
      <div className="event-detail-route__empty" style={EMPTY_WRAP}>
        <h1 style={EMPTY_TITLE}>Event not found</h1>
        <p style={EMPTY_SUB}>
          This event may have ended or the link is no longer valid.
        </p>
        <Link to={href("/whats-on")} style={EMPTY_LINK}>
          &#x2190; Back to What&apos;s On
        </Link>
      </div>
    </SitesChrome>
  );
}

const EMPTY_WRAP: React.CSSProperties = {
  maxWidth: 640,
  margin: "0 auto",
  padding: "96px 24px",
  color: "#fff",
  textAlign: "center",
};
const EMPTY_TITLE: React.CSSProperties = {
  fontSize: 32,
  fontWeight: 800,
  margin: "0 0 12px",
};
const EMPTY_SUB: React.CSSProperties = {
  fontSize: 16,
  color: "rgba(255,255,255,.7)",
  margin: "0 0 24px",
};
const EMPTY_LINK: React.CSSProperties = {
  color: "#ff2d55",
  fontWeight: 600,
  textDecoration: "none",
};
