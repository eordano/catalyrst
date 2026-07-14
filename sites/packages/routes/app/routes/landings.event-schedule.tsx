import { useNavigate } from "react-router";

import LdEventSchedulePage from "@ui/landings/pages/LdEventSchedulePage";

import {
  fetchSchedules,
  emptyDraft,
  scheduleToDraft,
  type Schedule,
  type ScheduleDraft,
} from "@data/lib/catalyst/landings/schedules";
import {
  fetchEvents,
  effectiveStartAt,
  toLiveNowCard,
  groupEventsByDay,
  type Event,
  type LiveNowCard,
  type DayEvent,
} from "@data/lib/catalyst/places/events";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import ScheduleWizard from "@features/stories/landings/event-schedule/ScheduleWizard";

import type { Route } from "./+types/landings.event-schedule";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "landings/event-schedule";

type ScheduleSource = "live" | "empty" | "error";

const FALLBACK: Assignment = {
  variant: "builder",
  flags: { builder: true },
  experimentKey: "lp_schedule_builder",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const mode = url.searchParams.get("mode") === "edit" ? "edit" : "create";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const editId = url.searchParams.get("id");

  let schedules: Schedule[] = [];
  let source: ScheduleSource;
  try {
    const { data: live } = await fetchSchedules({ signal: request.signal });
    schedules = live;
    source = live.length > 0 ? "live" : "empty";
  } catch {
    schedules = [];
    source = "error";
  }

  const now = new Date();
  const [liveEvents, activeEvents] = await Promise.all([
    fetchEvents({ list: "live", limit: 8 }, { signal: request.signal })
      .then((r) => r.data)
      .catch(() => [] as Event[]),
    fetchEvents({ list: "active", limit: 100 }, { signal: request.signal })
      .then((r) => r.data)
      .catch(() => [] as Event[]),
  ]);
  const sortedActive = [...activeEvents].sort(
    (a, b) =>
      new Date(effectiveStartAt(a, now) ?? 8.64e15).getTime() -
      new Date(effectiveStartAt(b, now) ?? 8.64e15).getTime(),
  );
  const liveNow: LiveNowCard[] = liveEvents.map(toLiveNowCard);
  const { allDays, dayLabels } = groupEventsByDay(
    sortedActive,
    new Set(liveEvents.map((e) => e.id)),
    7,
    now,
  );

  const editTarget =
    mode === "edit"
      ? ((editId ? schedules.find((s) => s.id === editId) : schedules[0]) ?? null)
      : null;

  const payload = {
    sid,
    mode,
    source,
    schedules,
    liveNow,
    allDays,
    dayLabels,
    draft:
      mode === "edit" ? (editTarget ? scheduleToDraft(editTarget) : emptyDraft()) : null,
    scheduleId: editTarget?.id ?? null,
    assignment,
  };

  return wrap(payload);
}

type LoaderData = {
  sid: string;
  mode: "create" | "edit";
  source: ScheduleSource;
  schedules: Schedule[];
  liveNow: LiveNowCard[];
  allDays: DayEvent[][];
  dayLabels: string[];
  draft: ScheduleDraft | null;
  scheduleId: string | null;
  assignment: Assignment;
};

export default function EventScheduleRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData as LoaderData;
  const navigate = useNavigate();

  return (
    <LdEventSchedulePage
      mode={d.mode}
      onModeClick={(mode, e) => {
        e.preventDefault();
        navigate(`?mode=${mode}`);
      }}
    >
      <ScheduleWizard
        trackCtx={{
          sid: d.sid,
          story: STORY,
          variant: d.assignment.variant,
          experimentKey: d.assignment.experimentKey,
        }}
        schedules={d.schedules}
        source={d.source}
        liveNow={d.liveNow}
        allDays={d.allDays}
        dayLabels={d.dayLabels}
        scheduleId={d.scheduleId ?? undefined}
        draft={d.draft ?? undefined}
      />
    </LdEventSchedulePage>
  );
}
