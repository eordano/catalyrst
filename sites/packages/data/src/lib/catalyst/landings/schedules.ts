import { z } from "zod";

import { getJSON } from "../client";
import type { GetOptions } from "../client";
import { apiOkOf, okDataTotalOf } from "../envelope";
import { ScheduleRecordSchema } from "../generated-schemas/events";
import type { ApiOk as RsApiOk } from "@ui/generated/catalyst/events/ApiOk";
import type { ScheduleRecord as RsSchedule } from "@ui/generated/catalyst/events/ScheduleRecord";
import { warnInvalid } from "../warn";

/**
 * Validation truth is catalyrst-events' `ScheduleRecord`. `background` and
 * `active` are non-null there, so a row that carries neither is not a schedule
 * with no colours that nobody switched on -- it is a row we did not understand,
 * and `parseSchedules` drops it instead of casting it back into the list.
 */
export { ScheduleRecordSchema as ScheduleSchema };

export type Schedule = z.infer<typeof ScheduleRecordSchema>;

export function parseSchedule(raw: unknown): Schedule | null {
  const r = ScheduleRecordSchema.safeParse(raw);
  if (r.success) return r.data;
  warnInvalid("Schedule", r.error.issues);
  return null;
}

export function parseSchedules(raw: unknown[]): Schedule[] {
  const out: Schedule[] = [];
  for (const row of raw ?? []) {
    const schedule = parseSchedule(row);
    if (schedule) out.push(schedule);
  }
  return out;
}

const ScheduleListEnvelope = okDataTotalOf(z.array(z.unknown()));
const ScheduleDetailEnvelope = apiOkOf(z.unknown());

export type _DriftScheduleListEnvelope = Assert<
  AssignableTo<RsApiOk<RsSchedule[]>, z.input<typeof ScheduleListEnvelope>>
>;
export type _DriftScheduleDetailEnvelope = Assert<
  AssignableTo<RsApiOk<RsSchedule>, z.input<typeof ScheduleDetailEnvelope>>
>;
export async function fetchSchedules(
  opts: GetOptions = {},
): Promise<{ data: Schedule[]; total: number }> {
  const raw = await getJSON<unknown>("/events/api/schedules", opts);
  const env = ScheduleListEnvelope.safeParse(raw);
  if (!env.success) {
    warnInvalid("Schedules envelope", env.error.issues);
    throw new Error("schedules response did not match the events envelope");
  }
  const data = parseSchedules(env.data.data);
  return { data, total: env.data.total ?? data.length };
}

export const SCHEDULE_BOUNDS = {
  nameMax: 50,
  descriptionMax: 255,
  imageMax: 255,
  maxBackgroundColors: 3,
} as const;

export const THEME_OPTIONS: { value: string; label: string }[] = [
  { value: "", label: "No theme (custom colors)" },
  { value: "mvmf_2022", label: "Metaverse Festival 2022" },
  { value: "mvfw_2023", label: "Metaverse Fashion Week 2023" },
  { value: "pride_2023", label: "Pride Week 2023" },
];

export type ScheduleDraft = {
  name: string;
  description: string;
  imageUrl: string;
  theme: string;
  background: string[];
  activeSinceDate: string;
  activeUntilDate: string;
  active: boolean;
};

export function emptyDraft(): ScheduleDraft {
  return {
    name: "",
    description: "",
    imageUrl: "",
    theme: "",
    background: ["#7B61FF", "#16141A"],
    activeSinceDate: "",
    activeUntilDate: "",
    active: true,
  };
}

export function isoToDateInput(iso: string | null): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return d.toISOString().slice(0, 10);
}

/** An unread `active` becomes `false` in the draft, never `true`: the editor
 *  must not pre-arm a schedule as live on the strength of a field it did not
 *  read. Same for `background` -- the wizard's own palette, not a colour the
 *  record never carried. */
export function scheduleToDraft(s: Schedule): ScheduleDraft {
  const base = emptyDraft();
  return {
    name: s.name,
    description: s.description ?? "",
    imageUrl: s.image ?? "",
    theme: s.theme ?? "",
    background: s.background?.length ? s.background : base.background,
    activeSinceDate: isoToDateInput(s.active_since),
    activeUntilDate: isoToDateInput(s.active_until),
    active: s.active === true,
  };
}

export type ScheduleUpsertBody = {
  schedule_id?: string;
  name: string;
  description: string | null;
  image: string | null;
  theme: string | null;
  background: string[];
  active_since: number;
  active_until: number;
  active: boolean;
  signed_at: number;
};

export function dateToEpochMs(date: string): number {
  if (!date) return 0;
  const d = new Date(`${date}T00:00:00.000Z`);
  return Number.isNaN(d.getTime()) ? 0 : d.getTime();
}

export function toUpsertBody(draft: ScheduleDraft, scheduleId?: string): ScheduleUpsertBody {
  return {
    ...(scheduleId ? { schedule_id: scheduleId } : {}),
    name: draft.name.trim(),
    description: draft.description.trim() || null,
    image: draft.imageUrl.trim() || null,
    theme: draft.theme || null,
    background: draft.background.filter((c) => c.trim().length > 0),
    active_since: dateToEpochMs(draft.activeSinceDate),
    active_until: dateToEpochMs(draft.activeUntilDate),
    active: draft.active,
    signed_at: Date.now(),
  };
}

export type DraftIssues = Partial<Record<keyof ScheduleDraft, string>>;

export function validateStep(step: string, draft: ScheduleDraft): DraftIssues {
  const issues: DraftIssues = {};
  if (step === "basics") {
    if (!draft.name.trim()) issues.name = "Schedule name is required";
    else if (draft.name.length > SCHEDULE_BOUNDS.nameMax)
      issues.name = `Name must be ${SCHEDULE_BOUNDS.nameMax} characters or fewer`;
    if (draft.description.length > SCHEDULE_BOUNDS.descriptionMax)
      issues.description = `Description must be ${SCHEDULE_BOUNDS.descriptionMax} characters or fewer`;
    if (draft.imageUrl.length > SCHEDULE_BOUNDS.imageMax)
      issues.imageUrl = `Image URL must be ${SCHEDULE_BOUNDS.imageMax} characters or fewer`;
    if (draft.background.filter((c) => c.trim()).length === 0)
      issues.background = "Pick at least one background color";
  }
  if (step === "dates") {
    if (!draft.activeSinceDate) issues.activeSinceDate = "Pick a start date";
    if (!draft.activeUntilDate) issues.activeUntilDate = "Pick an end date";
    if (
      draft.activeSinceDate &&
      draft.activeUntilDate &&
      dateToEpochMs(draft.activeUntilDate) < dateToEpochMs(draft.activeSinceDate)
    )
      issues.activeUntilDate = "End date must be on or after the start date";
  }
  return issues;
}

export function isStepValid(step: string, draft: ScheduleDraft): boolean {
  return Object.keys(validateStep(step, draft)).length === 0;
}

export const SubmitResultSchema = z.object({
  id: z.string(),
  active: z.boolean(),
});
export type SubmitResult = z.infer<typeof SubmitResultSchema>;

export async function simulateSubmitSchedule(
  draft: ScheduleDraft,
  opts: { scheduleId?: string; signal?: AbortSignal; delayMs?: number } = {},
): Promise<SubmitResult> {
  const body = toUpsertBody(draft, opts.scheduleId);
  if (!body.name) throw new Error("name is required");

  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, opts.delayMs ?? 500);
    opts.signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });

  const id =
    opts.scheduleId ??
    `local-sim-${Math.abs(hashString(body.name + body.active_since)).toString(16).padStart(12, "0").slice(0, 12)}`;
  return { id, active: body.active };
}

function hashString(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (Math.imul(h, 31) + s.charCodeAt(i)) | 0;
  return h;
}

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Assert<T extends true> = T;
export type _DriftSchedule = Assert<AssignableTo<RsSchedule, Schedule>>;
