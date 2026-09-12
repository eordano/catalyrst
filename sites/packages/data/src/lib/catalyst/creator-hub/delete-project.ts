import { z } from "zod";

import fixture from "./delete-project.data.json";

const LayoutSchema = z.object({
  rows: z.number(),
  cols: z.number(),
});

const SceneParcelsSchema = z.object({
  base: z.string(),
  parcels: z.array(z.string()),
});

export const ProjectSchema = z.object({
  id: z.string().min(1),
  path: z.string(),
  title: z.string(),
  description: z.string().nullable(),
  thumbnail: z.string().nullable(),
  layout: LayoutSchema,
  scene: SceneParcelsSchema,
  createdAt: z.number().nullable(),
  updatedAt: z.number().nullable(),
  publishedAt: z.number().nullable(),
  size: z.number().nullable(),
  grad: z.string(),
  published: z.boolean().nullable(),
  hasDeployments: z.boolean().nullable(),
});
export type Project = z.infer<typeof ProjectSchema>;

export const DeleteCopySchema = z.object({
  title: z.string(),
  files_checkbox: z.string(),
  files_warning: z.string(),
  cancel: z.string(),
  confirm: z.string(),
  menu_delete: z.string(),
});
export type DeleteCopy = z.infer<typeof DeleteCopySchema>;

export const DeleteProjectDataSchema = z.object({
  copy: DeleteCopySchema,
  projects: z.array(ProjectSchema),
});
export type DeleteProjectData = z.infer<typeof DeleteProjectDataSchema>;

export function deleteCopy(): DeleteCopy {
  return DeleteCopySchema.parse((fixture as { copy: unknown }).copy);
}

export type CreatorSceneLike = {
  id: string;
  title: string;
  image?: string | null;
  base_position?: string;
  parcels?: number;
};

function sceneHue(id: string): number {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) % 360;
  return h;
}

export function sceneToProject(s: CreatorSceneLike): Project {
  const parcels = Math.max(1, s.parcels ?? 1);
  const h = sceneHue(s.id);
  return ProjectSchema.parse({
    id: s.id,
    path: s.base_position ? `Parcel ${s.base_position}` : s.title,
    title: s.title || "Untitled scene",
    description: null,
    thumbnail: s.image ?? null,
    layout: { rows: 1, cols: parcels },
    scene: { base: s.base_position ?? "0,0", parcels: [] },
    createdAt: null,
    updatedAt: null,
    publishedAt: null,
    size: null,
    published: null,
    hasDeployments: null,
    grad: `linear-gradient(135deg, hsl(${h} 70% 52%), hsl(${(h + 40) % 360} 60% 28%))`,
  });
}

export function parcelCount(p: Project): number {
  return Math.max(1, p.layout.cols * p.layout.rows);
}

