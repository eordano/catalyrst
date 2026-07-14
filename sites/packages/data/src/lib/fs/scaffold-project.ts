import {
  buildCompositeFromHierarchy,
  serializeSceneComposite,
} from "../catalyst/creator-hub/scene-composite";
import { newSceneSeed, layoutToParcels } from "../catalyst/creator-hub/scene-editor";
import {
  buildTemplateCompositeText,
  hasTemplateComposite,
  templateContentMeta,
  templateIndexTs,
} from "./template-composites";
import { saveTextFile, type SaveResult as DiskSaveResult } from "./disk";

export const SCENE_JSON_FILENAME = "scene.json";
export const COMPOSITE_FILENAME = "main.composite";

export type ScaffoldFileContent = { path: string; text: string };

export type BuildScaffoldOptions = {
  name?: string;
  template?: string;
  templateTitle?: string;
  githubLink?: string;
  layout?: string | null;
  parcels?: string[];
};

export const TEMPLATE_FETCH_ENABLED = false;

export type TemplateContent = {
  composite?: unknown;
  indexTs?: string;
  extraFiles?: ScaffoldFileContent[];
};

export async function fetchTemplateContent(
  _githubLink: string | undefined,
  opts: { enabled?: boolean; fetchImpl?: typeof fetch; signal?: AbortSignal } = {},
): Promise<TemplateContent | null> {
  const enabled = opts.enabled ?? TEMPLATE_FETCH_ENABLED;
  if (!enabled) return null;
  return null;
}

export function projectSlug(name: string | undefined): string {
  return (
    (name ?? "")
      .toLowerCase()
      .trim()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "new-scene"
  );
}

export function buildScaffoldFiles(opts: BuildScaffoldOptions = {}): ScaffoldFileContent[] {
  const name = (opts.name ?? "").trim() || "My Awesome Scene";
  const template = (opts.template ?? "empty").trim() || "empty";
  const contentMeta = templateContentMeta(template);
  const templateTitle =
    (opts.templateTitle ?? "").trim() ||
    contentMeta?.title ||
    (template === "empty" ? "Empty Scene" : template);
  const githubLink = opts.githubLink || contentMeta?.githubLink;
  const parcels =
    opts.parcels && opts.parcels.length ? opts.parcels : layoutToParcels(opts.layout);
  const base = parcels[0] ?? "0,0";

  const templateCompositeText = hasTemplateComposite(template)
    ? buildTemplateCompositeText(template)
    : null;
  const seed = newSceneSeed({ name, template, parcels });
  const compositeText =
    templateCompositeText ??
    serializeSceneComposite(buildCompositeFromHierarchy(seed.hierarchy));

  const sceneJson = {
    ecs7: true,
    runtimeVersion: "7",
    display: {
      title: name,
      description:
        template === "empty"
          ? "A new Decentraland scene."
          : `A new Decentraland scene created from the ${templateTitle} template.`,
      navmapThumbnail: "images/scene-thumbnail.png",
    },
    owner: "",
    contact: { name: "", email: "" },
    main: "bin/index.js",
    tags: template === "empty" ? [] : [template],
    scene: { parcels, base },
    spawnPoints: [
      {
        name: "spawn1",
        default: true,
        position: { x: [0, 3], y: [0, 0], z: [0, 3] },
        cameraTarget: { x: 8, y: 1, z: 8 },
      },
    ],
    featureToggles: { voiceChat: "enabled", portableExperiences: "enabled" },
  };
  const sceneJsonText = JSON.stringify(sceneJson, null, 2);

  const packageJson = {
    name: projectSlug(name),
    version: "1.0.0",
    description: sceneJson.display.description,
    scripts: {
      start: "sdk-commands start",
      deploy: "sdk-commands deploy",
      build: "sdk-commands build",
      upgrade: "npm install @dcl/sdk@latest",
    },
    devDependencies: { "@dcl/sdk": "latest", "@dcl/asset-packs": "latest" },
  };

  const tsconfig = {
    compilerOptions: {
      incremental: true,
      lib: ["es2020", "dom"],
      strict: true,
      target: "es2020",
      module: "es2020",
      moduleResolution: "node",
      esModuleInterop: true,
      outDir: "bin",
    },
    extends: "@dcl/sdk/types/tsconfig.ecs7.json",
  };

  const indexTs =
    templateIndexTs(template, name) ??
    `import { engine, Transform } from '@dcl/sdk/ecs'\n` +
      `import { Vector3 } from '@dcl/sdk/math'\n` +
      `import { initAssetPacks } from '@dcl/asset-packs/dist/scene-entrypoint'\n\n` +
      `// ${name} \u{2014} scaffolded by the Decentraland Creator Hub.\n` +
      `// initAssetPacks runs the no-code Actions/Triggers (smart items) authored in the\n` +
      `// editor; without it they decode but never execute. (sdk-commands only auto-injects\n` +
      `// it for the assets/scene/main.composite layout, and this project keeps the\n` +
      `// composite at the root.)\n` +
      `initAssetPacks(engine)\n\n` +
      `export function main() {\n` +
      `  const root = engine.addEntity()\n` +
      `  Transform.create(root, { position: Vector3.create(8, 0, 8) })\n` +
      `}\n`;

  const gitignore = `node_modules/\nbin/\n.DS_Store\n*.log\n`;

  const readme =
    `# ${name}\n\n` +
    `${sceneJson.display.description}\n\n` +
    `- **Parcels:** ${parcels.join(", ")}\n` +
    `- **Base:** ${base}\n` +
    `- **Template:** ${templateTitle} (\`${template}\`)\n` +
    (githubLink ? `- **Original scene:** ${githubLink}\n` : "") +
    (contentMeta
      ? `\n## What this scaffold contains\n\n${contentMeta.readmeNote}\n\n` +
        `The placed models are referenced under \`assets/imported/template-assets/\` ` +
        `and come from the public builder catalog; the web editor writes them into ` +
        `the project on Save (publishing fetches any that are still missing). Every ` +
        `entity in \`main.composite\` is editable and deletable in the Scene Editor.\n`
      : "") +
    `\n## Develop\n\n` +
    "```bash\nnpm install\nnpm start\n```\n";

  return [
    { path: SCENE_JSON_FILENAME, text: sceneJsonText },
    { path: COMPOSITE_FILENAME, text: compositeText },
    { path: "package.json", text: JSON.stringify(packageJson, null, 2) },
    { path: "tsconfig.json", text: JSON.stringify(tsconfig, null, 2) },
    { path: "src/index.ts", text: indexTs },
    { path: ".gitignore", text: gitignore },
    { path: "README.md", text: readme },
  ];
}


type DirHandle = FileSystemDirectoryHandle & {
  getDirectoryHandle: (
    name: string,
    opts?: { create?: boolean },
  ) => Promise<DirHandle>;
  getFileHandle: (
    name: string,
    opts?: { create?: boolean },
  ) => Promise<FileSystemFileHandle>;
};

async function fileHandleAt(
  dir: DirHandle,
  relPath: string,
): Promise<FileSystemFileHandle> {
  const parts = relPath.split("/").filter(Boolean);
  let cur = dir;
  for (let i = 0; i < parts.length - 1; i += 1) {
    cur = await cur.getDirectoryHandle(parts[i], { create: true });
  }
  return cur.getFileHandle(parts[parts.length - 1], { create: true });
}

async function writeInto(handle: FileSystemFileHandle, text: string): Promise<void> {
  const create = (handle as unknown as {
    createWritable: () => Promise<{
      write: (d: string) => Promise<void>;
      close: () => Promise<void>;
    }>;
  }).createWritable;
  const writable = await create.call(handle);
  await writable.write(text);
  await writable.close();
}

export type ScaffoldWriteResult = {
  written: boolean;
  via: "directory" | "download" | "canceled";
  folder: string;
  paths: string[];
  dir?: FileSystemDirectoryHandle | null;
};

export type WriteScaffoldOptions = {
  name?: string;
  dir?: DirHandle | null;
  downloadWriter?: (name: string, text: string) => Promise<DiskSaveResult>;
  forceDownload?: boolean;
};

type DirPickerWindow = Window & {
  showDirectoryPicker?: (opts?: {
    mode?: "read" | "readwrite";
  }) => Promise<DirHandle>;
};

function isAbort(e: unknown): boolean {
  return !!e && typeof e === "object" && (e as DOMException).name === "AbortError";
}

export async function writeScaffoldFiles(
  files: ScaffoldFileContent[],
  opts: WriteScaffoldOptions = {},
): Promise<ScaffoldWriteResult> {
  const folder = projectSlug(opts.name);
  const w = typeof window === "undefined" ? null : (window as DirPickerWindow);

  if (!opts.forceDownload) {
    let parent: DirHandle | null = opts.dir ?? null;
    if (!parent && w && typeof w.showDirectoryPicker === "function") {
      try {
        parent = await w.showDirectoryPicker({ mode: "readwrite" });
      } catch (e) {
        if (isAbort(e)) return { written: false, via: "canceled", folder, paths: [] };
        parent = null;
      }
    }
    if (parent) {
      const dir = await parent.getDirectoryHandle(folder, { create: true });
      for (const f of files) {
        const handle = await fileHandleAt(dir, f.path);
        await writeInto(handle, f.text);
      }
      return {
        written: true,
        via: "directory",
        folder,
        paths: files.map((f) => f.path),
        dir,
      };
    }
  }

  if (opts.downloadWriter) {
    for (const f of files) {
      await opts.downloadWriter(f.path.replace(/\//g, "-"), f.text);
    }
    return { written: true, via: "download", folder, paths: files.map((f) => f.path) };
  }
  const { makeZip } = await import("./zip");
  const zip = makeZip(files.map((f) => ({ path: `${folder}/${f.path}`, text: f.text })));
  downloadZipBlob(`${folder}.zip`, zip);
  return { written: true, via: "download", folder, paths: files.map((f) => f.path) };
}

function downloadZipBlob(name: string, bytes: Uint8Array): void {
  if (typeof document === "undefined") return;
  const buf = bytes.slice().buffer as ArrayBuffer;
  const url = URL.createObjectURL(new Blob([buf], { type: "application/zip" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.rel = "noopener";
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
