import LdSubmitHangoutView from "@ui/landings/pages/LdSubmitHangoutView";

import { fetchEventCategories, type EventCategory } from "@data/lib/catalyst/places/events";
import { emptyDraft, type HangoutDraft } from "@data/lib/catalyst/landings/submit-hangout";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import SubmitHangout, {
  type CategoryOption,
} from "@features/stories/landings/submit-hangout/SubmitHangout";

import type { Route } from "./+types/landings.submit-hangout";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "landings/submit-hangout";

function toOption(c: EventCategory): CategoryOption {
  return { name: c.name, label: c.i18n?.en ?? c.name };
}

function sampleEditDraft(): HangoutDraft {
  return {
    ...emptyDraft(),
    name: "Neon Nights \u{2014} Synthwave Live Set",
    description:
      "Join us for a two-hour synthwave & retrowave DJ set in our rooftop club. Grab a drink, dance, and meet other music lovers. Doors open at the start time \u{2014} come early for the best spot on the floor.",
    startDate: "2026-07-18",
    startTime: "20:00",
    durationHours: 2,
    allDay: false,
    location: "land",
    coordX: -45,
    coordY: 120,
    worldName: "",
    recurrent: true,
    recurrence: "every_week",
    recurrentUntil: "2026-09-26",
    category: "music",
    communityId: "music",
    contact: "host@neon-nights.xyz",
  };
}

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "lp_hangout_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const mode = url.searchParams.get("mode") === "edit" ? "edit" : "create";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  let categoriesError = false;
  const categories: CategoryOption[] = await fetchEventCategories({
    signal: request.signal,
  })
    .then((cats) => cats.map(toOption))
    .catch(() => {
      categoriesError = true;
      return [];
    });

  const payload = {
    sid,
    mode,
    categories,
    categoriesError,
    draft: mode === "edit" ? sampleEditDraft() : null,
    assignment,
  };

  return wrap(payload);
}

type LoaderData = {
  sid: string;
  mode: "create" | "edit";
  categories: CategoryOption[];
  categoriesError: boolean;
  draft: HangoutDraft | null;
  assignment: Assignment;
};

export default function SubmitHangoutRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData as LoaderData;

  return (
    <LdSubmitHangoutView>
      <SubmitHangout
        trackCtx={{
          sid: d.sid,
          story: STORY,
          variant: d.assignment.variant,
          experimentKey: d.assignment.experimentKey,
        }}
        categories={d.categories}
        categoriesError={d.categoriesError}
        mode={d.mode}
        draft={d.draft ?? undefined}
      />
    </LdSubmitHangoutView>
  );
}
