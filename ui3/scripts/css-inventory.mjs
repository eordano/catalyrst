import { readFileSync, readdirSync } from "node:fs";
import { resolve, relative, basename } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";

const repo = resolve(fileURLToPath(new URL("../../..", import.meta.url)));
const roots = ["catalyrst/ui3/src", "catalyrst/sites/packages", "catalyrst/social/src"];
function walk(path) {
  return readdirSync(path, { withFileTypes: true }).flatMap(entry => {
    const child = resolve(path, entry.name);
    return entry.isDirectory() ? walk(child) : [child];
  });
}
const sources = roots.flatMap(root => walk(resolve(repo, root)))
  .filter(path => /\.(?:css|[cm]?[jt]sx?|html|md)$/.test(path))
  .map(path => ({ path: relative(repo, path), content: readFileSync(path, "utf8") }));
const styles = sources.filter(source => source.path.endsWith(".css"));
const identical = new Map();
const files = styles.map(({ path, content }) => {
  const hash = createHash("sha256").update(content).digest("hex");
  identical.set(hash, [...(identical.get(hash) ?? []), path]);
  const references = sources.filter(other => other.path !== path && other.content.includes(basename(path))).map(other => other.path);
  return { path, bytes: Buffer.byteLength(content), lines: content.split("\n").length - 1, references };
}).sort((a, b) => b.bytes - a.bytes);
console.log(JSON.stringify({
  scope: roots,
  totals: { files: files.length, bytes: files.reduce((sum, file) => sum + file.bytes, 0), lines: files.reduce((sum, file) => sum + file.lines, 0) },
  identicalFiles: [...identical.values()].filter(paths => paths.length > 1),
  unreferencedStylesheets: files.filter(file => !file.references.length),
  note: "Unreferenced files are review candidates, not proof of unused selectors. Check generated imports, public exports and external consumers before removal. Bytes measure source CSS, not compressed transfer size.",
  files,
}, null, 2));
