import { z } from "zod";

export type RecurrenceOption = {
  value: string;
  label: string;
  frequency: "YEARLY" | "MONTHLY" | "WEEKLY" | "DAILY" | "HOURLY";
  interval: number;
};

export const HANGOUT_BOUNDS = {
  nameMax: 250,
  descriptionMax: 5000,
  coordMin: -150,
  coordMax: 150,
  maxRecurrent: 10,
  defaultDurationMs: 3_600_000,
} as const;

export type HangoutDraft = {
  name: string;
  description: string;
  startDate: string;
  startTime: string;
  durationHours: number;
  allDay: boolean;
  location: "land" | "world";
  coordX: number;
  coordY: number;
  worldName: string;
  recurrent: boolean;
  recurrence: string;
  recurrentUntil: string;
  category: string;
  communityId: string;
  contact: string;
  details: string;
  imagePreviewUrl: string | null;
  verticalImagePreviewUrl: string | null;
};

export function emptyDraft(): HangoutDraft {
  return {
    name: "",
    description: "",
    startDate: "",
    startTime: "",
    durationHours: 2,
    allDay: false,
    location: "land",
    coordX: 0,
    coordY: 0,
    worldName: "",
    recurrent: false,
    recurrence: "every_week",
    recurrentUntil: "",
    category: "",
    communityId: "",
    contact: "",
    details: "",
    imagePreviewUrl: null,
    verticalImagePreviewUrl: null,
  };
}

export type CreateEventBody = {
  name: string;
  description: string;
  start_at: string | null;
  finish_at: string | null;
  x: number;
  y: number;
};

export function toIso(date: string, time: string): string | null {
  if (!date) return null;
  const t = time && /^\d{1,2}:\d{2}$/.test(time) ? time : "00:00";
  const d = new Date(`${date}T${t}:00.000Z`);
  return Number.isNaN(d.getTime()) ? null : d.toISOString();
}

export function toCreateEventBody(draft: HangoutDraft): CreateEventBody {
  const start = toIso(draft.startDate, draft.startTime);
  let finish: string | null = null;
  if (start) {
    const ms = new Date(start).getTime() + draft.durationHours * 3_600_000;
    finish = new Date(ms).toISOString();
  }
  return {
    name: draft.name.trim(),
    description: draft.description.trim(),
    start_at: start,
    finish_at: finish,
    x: draft.location === "world" ? 0 : draft.coordX,
    y: draft.location === "world" ? 0 : draft.coordY,
  };
}

export type DraftIssues = Partial<Record<keyof HangoutDraft, string>>;

export function validateStep(step: string, draft: HangoutDraft): DraftIssues {
  const issues: DraftIssues = {};
  if (step === "details") {
    if (!draft.name.trim()) issues.name = "Hangout name is required";
    else if (draft.name.length > HANGOUT_BOUNDS.nameMax)
      issues.name = `Name must be ${HANGOUT_BOUNDS.nameMax} characters or fewer`;
    if (draft.description.length > HANGOUT_BOUNDS.descriptionMax)
      issues.description = `Description must be ${HANGOUT_BOUNDS.descriptionMax} characters or fewer`;
  }
  if (step === "location") {
    if (draft.location === "world") {
      if (!draft.worldName) issues.worldName = "Select a world";
    } else {
      const { coordMin, coordMax } = HANGOUT_BOUNDS;
      if (draft.coordX < coordMin || draft.coordX > coordMax)
        issues.coordX = `X must be between ${coordMin} and ${coordMax}`;
      if (draft.coordY < coordMin || draft.coordY > coordMax)
        issues.coordY = `Y must be between ${coordMin} and ${coordMax}`;
    }
  }
  if (step === "schedule") {
    if (!draft.startDate) issues.startDate = "Pick a date";
    if (!draft.allDay && !draft.startTime) issues.startTime = "Pick a start time";
    if (draft.recurrent && !draft.recurrentUntil)
      issues.recurrentUntil = "Pick an end date for the recurrence";
  }
  return issues;
}

export function isStepValid(step: string, draft: HangoutDraft): boolean {
  return Object.keys(validateStep(step, draft)).length === 0;
}

export const SubmitResultSchema = z.object({
  id: z.string(),
  approved: z.boolean(),
});
export type SubmitResult = z.infer<typeof SubmitResultSchema>;

export async function simulateSubmitHangout(
  draft: HangoutDraft,
  opts: { signal?: AbortSignal; delayMs?: number } = {},
): Promise<SubmitResult> {
  const body = toCreateEventBody(draft);
  if (!body.name) throw new Error("name is required");

  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, opts.delayMs ?? 500);
    opts.signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });

  const id = `local-sim-${Math.abs(hashString(body.name + (body.start_at ?? ""))).toString(16).padStart(12, "0").slice(0, 12)}`;
  return { id, approved: false };
}

function hashString(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (Math.imul(h, 31) + s.charCodeAt(i)) | 0;
  return h;
}
