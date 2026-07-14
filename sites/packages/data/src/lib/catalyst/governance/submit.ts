import { z } from "zod";

import fixture from "../../../fixtures/governance-submit-proposal.json";

const ChooserOptionSchema = z.object({
  request: z.enum(["add", "remove"]),
  type: z.string(),
  title: z.string(),
  description: z.string(),
  active: z.boolean(),
  paused: z.string().nullish(),
  route: z.string(),
});

const ChooserSchema = z.object({
  title: z.string(),
  prompt: z.string(),
  options: z.array(ChooserOptionSchema),
});

const CategorySchema = z.object({
  type: z.string(),
  title: z.string(),
  description: z.string(),
  active: z.boolean(),
  isNew: z.boolean().nullish(),
  behavior: z.enum(["link", "chooser"]),
  route: z.string().nullish(),
  choiceTitle: z.string().nullish(),
  paused: z.string().nullish(),
});

const GroupSchema = z.object({
  id: z.string(),
  heading: z.string(),
  categories: z.array(CategorySchema),
});

const SubmitHubSchema = z.object({
  page: z.object({ title: z.string(), lead: z.string() }),
  groups: z.array(GroupSchema),
  choosers: z.record(z.string(), ChooserSchema),
});

export type SubmitChooser = z.infer<typeof ChooserSchema>;
export type SubmitGroup = z.infer<typeof GroupSchema>;
export type SubmitHub = z.infer<typeof SubmitHubSchema>;

const FALLBACK: SubmitHub = {
  page: { title: "Submit Proposal", lead: "Select a proposal category to get started" },
  groups: [
    {
      id: "common",
      heading: "Common Actions",
      categories: [
        {
          type: "poll",
          title: "Poll",
          description: "Ask community members for their opinion on an issue or topic",
          active: true,
          behavior: "link",
          route: "/governance/submit/poll",
        },
      ],
    },
  ],
  choosers: {},
};

function parseHub(): SubmitHub {
  const parsed = SubmitHubSchema.safeParse(fixture);
  return parsed.success ? parsed.data : FALLBACK;
}

export function getSubmitHub(): SubmitHub {
  return parseHub();
}

export function getGroups(groupId?: string): SubmitGroup[] {
  const groups = parseHub().groups;
  if (!groupId) return groups;
  const filtered = groups.filter((g) => g.id === groupId);
  return filtered.length > 0 ? filtered : groups;
}

export function getGroupIds(): string[] {
  return parseHub().groups.map((g) => g.id);
}

export function getChooser(type: string): SubmitChooser | null {
  return parseHub().choosers[type] ?? null;
}

export function getChooserTypes(): string[] {
  return Object.keys(parseHub().choosers);
}
