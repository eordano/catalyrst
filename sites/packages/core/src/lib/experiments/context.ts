import fs from "node:fs";
import path from "node:path";

import matter from "gray-matter";
import { z } from "zod";

export const VariantSchema = z.object({
  id: z.string().min(1),
  weight: z.number().nonnegative(),
  flags: z.record(z.string(), z.unknown()).default({}),
});
export type Variant = z.infer<typeof VariantSchema>;

export const ExperimentSchema = z.object({
  key: z.string().min(1),
  unit: z.string().min(1),
  variants: z.array(VariantSchema).min(1),
  baseline: z.number().optional(),
  mde: z.number().optional(),
  min_sample: z.number().optional(),
});

export const HypothesisSchema = z.object({
  statement: z.string(),
  because: z.string(),
});

export const MetricSchema = z.object({
  primary: z.string(),
  guardrails: z.array(z.string()).default([]),
});

export const DecisionSchema = z.object({
  rule: z.string(),
});

export const StoryMetaSchema = z.object({
  id: z.string().min(1),
  status: z.string().min(1),
  owner: z.string().min(1),
  hypothesis: HypothesisSchema,
  metric: MetricSchema,
  experiment: ExperimentSchema,
  decision: DecisionSchema,
});
export type StoryMeta = z.infer<typeof StoryMetaSchema>;

export class StoryParseError extends Error {
  constructor(
    message: string,
    readonly cause?: unknown,
  ) {
    super(message);
    this.name = "StoryParseError";
  }
}

const storyCache = new Map<string, StoryMeta>();

export function parseStory(storyDir: string): StoryMeta {
  const cached = storyCache.get(storyDir);
  if (cached) return cached;
  const meta = parseStoryUncached(storyDir);
  storyCache.set(storyDir, meta);
  return meta;
}

function parseStoryUncached(storyDir: string): StoryMeta {
  const file = path.join(storyDir, "story.md");

  let raw: string;
  try {
    raw = fs.readFileSync(file, "utf8");
  } catch (cause) {
    throw new StoryParseError(`Cannot read story file: ${file}`, cause);
  }

  let data: unknown;
  try {
    data = matter(raw).data;
  } catch (cause) {
    throw new StoryParseError(`Invalid frontmatter in ${file}`, cause);
  }

  const parsed = StoryMetaSchema.safeParse(data);
  if (!parsed.success) {
    throw new StoryParseError(
      `story.md frontmatter failed validation (${file}): ${parsed.error.message}`,
      parsed.error,
    );
  }
  return parsed.data;
}

export function parseStoryContent(content: string, label = "<inline>"): StoryMeta {
  let data: unknown;
  try {
    data = matter(content).data;
  } catch (cause) {
    throw new StoryParseError(`Invalid frontmatter in ${label}`, cause);
  }
  const parsed = StoryMetaSchema.safeParse(data);
  if (!parsed.success) {
    throw new StoryParseError(
      `story.md frontmatter failed validation (${label}): ${parsed.error.message}`,
      parsed.error,
    );
  }
  return parsed.data;
}
