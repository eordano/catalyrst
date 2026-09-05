import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

(globalThis as unknown as { window: unknown }).window = {};

const root = mkdtempSync(join(tmpdir(), "scaffold-"));
const evidenceDir = "/tmp/scaffold-project-evidence";
mkdirSync(evidenceDir, { recursive: true });

function dirHandle(abs: string): unknown {
  return {
    getDirectoryHandle: async (name: string) => {
      const p = join(abs, name);
      mkdirSync(p, { recursive: true });
      return dirHandle(p);
    },
    getFileHandle: async (name: string) => {
      const p = join(abs, name);
      return {
        createWritable: async () => {
          let buf = "";
          return {
            write: async (d: string) => {
              buf += d;
            },
            close: async () => {
              writeFileSync(p, buf, "utf8");
            },
          };
        },
      };
    },
  };
}

const { buildScaffoldFiles, writeScaffoldFiles } = await import(
  "../packages/data/src/lib/fs/scaffold-project"
);
const { parseComposite, entityName, listEntities } = await import(
  "../packages/data/src/lib/catalyst/scene-composite.ts"
);

const NAME = "My Tavern Scene";
const TEMPLATE = "tower-defense";

const files = buildScaffoldFiles({
  name: NAME,
  template: TEMPLATE,
  templateTitle: "Tower Defense",
  githubLink: "https://github.com/decentraland-scenes/Tower-defense",
  layout: "2x2",
});

const res = await writeScaffoldFiles(files, {
  name: NAME,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  dir: dirHandle(root) as any,
});

const projectDir = join(root, res.folder);

const sceneJsonText = readFileSync(join(projectDir, "scene.json"), "utf8");
const compositeText = readFileSync(join(projectDir, "main.composite"), "utf8");
writeFileSync(join(evidenceDir, "scene.json"), sceneJsonText, "utf8");
writeFileSync(join(evidenceDir, "main.composite"), compositeText, "utf8");

const sj = JSON.parse(sceneJsonText);
const comp = parseComposite(JSON.parse(compositeText));

const checks: [string, boolean][] = [];
const eq = (n: string, c: boolean) => checks.push([n, c]);

eq("write reported as a real in-place directory write", res.written === true && res.via === "directory");
eq("project folder slugged from the typed name", res.folder === "my-tavern-scene");
eq("scene.json exists on disk", existsSync(join(projectDir, "scene.json")));
eq("main.composite exists on disk", existsSync(join(projectDir, "main.composite")));
eq("nested src/index.ts written on disk", existsSync(join(projectDir, "src", "index.ts")));
eq("scene.json display.title is the USER-TYPED name", sj.display.title === NAME);
eq("scene.json is SDK7 (ecs7 + runtimeVersion)", sj.ecs7 === true && sj.runtimeVersion === "7");
eq("scene.json parcels match the 2x2 layout", JSON.stringify(sj.scene.parcels) === JSON.stringify(["0,0", "1,0", "0,1", "1,1"]));
eq("scene.json records the chosen template id", Array.isArray(sj.tags) && sj.tags.includes(TEMPLATE));
eq("main.composite is a valid composite (version 1)", comp.version === 1);
eq("main.composite carries starter entity 'Ground' (#512)", listEntities(comp).includes(512) && entityName(comp, 512) === "Ground");

const readme = readFileSync(join(projectDir, "README.md"), "utf8");
eq("README references the template's github scene", readme.includes("https://github.com/decentraland-scenes/Tower-defense"));

let ok = true;
for (const [n, c] of checks) {
  console.log(`${c ? "PASS" : "FAIL"}  ${n}`);
  if (!c) ok = false;
}

console.log("\n--- write result ---");
console.log(JSON.stringify(res, null, 2));
console.log(`\nproject written to: ${projectDir}`);
console.log(`evidence: ${evidenceDir}/scene.json  ${evidenceDir}/main.composite`);

if (!ok) {
  console.error("\nPROOF FAILED");
  process.exit(1);
}
console.log("\nPROOF OK \u{2014} real scaffold written to disk with the typed name + layout + starter composite.");
