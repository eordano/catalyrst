import {
  deleteCopy,
  sceneToProject,
  type DeleteProjectData,
  type Project,
} from "./delete-project";
import { loadCreatorScenes } from "../create/index.server";

export type LoadDeleteProjectResult = {
  data: DeleteProjectData;
  source: "live" | "empty" | "unavailable";
};

export async function loadDeleteProjectData(
  opts: { creator?: string } = {},
): Promise<LoadDeleteProjectResult> {
  const creator = opts.creator?.trim();
  let projects: Project[] = [];
  let unavailable = false;
  try {
    const scenes = await loadCreatorScenes({
      creator: creator || undefined,
      limit: 60,
    });
    projects = scenes.map(sceneToProject);
  } catch {
    unavailable = true;
  }
  return {
    data: { copy: deleteCopy(), projects },
    source: unavailable ? "unavailable" : creator ? "live" : "empty",
  };
}
