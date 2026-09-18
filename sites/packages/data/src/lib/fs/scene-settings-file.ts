interface TextProject {
  read(path: string): Promise<string>;
  readOnly?(path: string): Promise<string>;
  write(path: string, content: string): Promise<void>;
  createFileSession?(): TextProject;
}

export interface SettingsFiles {
  project?: TextProject;
  getDir?: () => Promise<FileSystemDirectoryHandle | null>;
  virtualFiles?: { path: string; text: string }[];
  hydrate?: () => Promise<Record<string, string> | null>;
  persist?: (path: string, text: string) => Promise<void> | void;
}

export async function openSceneSettingsFile(files: SettingsFiles) {
  const path = "scene.json";
  let project: TextProject;
  let destination: string;
  if (files.project) {
    project = files.project.createFileSession?.() ?? files.project;
    destination = "SDK project";
  } else {
    const dir = await files.getDir?.();
    if (dir) {
      project = {
        read: async () => (await (await dir.getFileHandle(path)).getFile()).text(),
        write: async (_path, content) => {
          const stream = await (await dir.getFileHandle(path)).createWritable();
          try { await stream.write(content); await stream.close(); }
          catch (error) { await stream.abort().catch(() => {}); throw error; }
        },
      };
      destination = "Project folder";
    } else {
      if (!files.hydrate || !files.persist) throw new Error("Open a project before changing scene settings.");
      const scaffold = files.virtualFiles?.find(file => file.path === path)?.text;
      project = {
        read: async () => {
          const value = (await files.hydrate!())?.[path] ?? scaffold;
          if (value === undefined) throw new Error("This project has no scene settings file.");
          return value;
        },
        write: async (_path, content) => {
          await files.persist!(path, content);
          if ((await files.hydrate!())?.[path] !== content) throw new Error("Your browser could not save scene settings. Free some storage and try again.");
        },
      };
      destination = "Browser draft";
    }
  }
  let baseline = await project.read(path);
  return {
    content: baseline, destination,
    async save(content: string) {
      if (await (project.readOnly ? project.readOnly(path) : project.read(path)) !== baseline) throw new Error("Scene settings changed outside this form. Reload settings before saving.");
      await project.write(path, content);
      baseline = content;
    },
  };
}
