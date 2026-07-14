type ProcessLike = { cwd?: () => string; env?: Record<string, string | undefined> };
const g = globalThis as unknown as { process?: ProcessLike };
if (!g.process) g.process = { cwd: () => "/", env: {} };
else if (typeof g.process.cwd !== "function") g.process.cwd = () => "/";

export const sep = "/";
export const delimiter = ":";

export function normalize(p: string): string {
  const abs = p.startsWith("/");
  const out: string[] = [];
  for (const part of p.split("/")) {
    if (!part || part === ".") continue;
    if (part === ".." && out.length && out[out.length - 1] !== "..") out.pop();
    else out.push(part);
  }
  const body = out.join("/");
  return abs ? `/${body}` : body || ".";
}

export function join(...parts: string[]): string {
  const joined = parts.filter(Boolean).join("/");
  return joined ? normalize(joined) : ".";
}

export function resolve(...parts: string[]): string {
  let acc = "/";
  for (const part of parts) {
    if (!part) continue;
    acc = part.startsWith("/") ? part : `${acc}/${part}`;
  }
  return normalize(acc);
}

export function dirname(p: string): string {
  const s = p.replace(/\/+$/, "");
  const i = s.lastIndexOf("/");
  if (i < 0) return ".";
  if (i === 0) return "/";
  return s.slice(0, i);
}

export function basename(p: string, ext?: string): string {
  const s = p.replace(/\/+$/, "");
  let b = s.slice(s.lastIndexOf("/") + 1);
  if (ext && b.endsWith(ext) && b !== ext) b = b.slice(0, -ext.length);
  return b;
}

export function extname(p: string): string {
  const b = basename(p);
  const i = b.lastIndexOf(".");
  return i <= 0 ? "" : b.slice(i);
}

export function isAbsolute(p: string): boolean {
  return p.startsWith("/");
}

export function relative(from: string, to: string): string {
  const f = normalize(resolve(from)).split("/").filter(Boolean);
  const t = normalize(resolve(to)).split("/").filter(Boolean);
  let i = 0;
  while (i < f.length && i < t.length && f[i] === t[i]) i++;
  return [...f.slice(i).map(() => ".."), ...t.slice(i)].join("/");
}

export function parse(p: string) {
  const dir = dirname(p);
  const base = basename(p);
  const ext = extname(p);
  return { root: isAbsolute(p) ? "/" : "", dir, base, ext, name: ext ? base.slice(0, -ext.length) : base };
}

export function format(o: { dir?: string; root?: string; base?: string; name?: string; ext?: string }): string {
  const base = o.base ?? `${o.name ?? ""}${o.ext ?? ""}`;
  const dir = o.dir ?? o.root ?? "";
  return dir ? `${dir}/${base}` : base;
}

export function toNamespacedPath(p: string): string {
  return p;
}

const pathShim = {
  sep,
  delimiter,
  normalize,
  join,
  resolve,
  dirname,
  basename,
  extname,
  isAbsolute,
  relative,
  parse,
  format,
  toNamespacedPath,
  posix: undefined as unknown,
  win32: undefined as unknown,
  default: undefined as unknown,
};
pathShim.posix = pathShim;
pathShim.win32 = pathShim;
pathShim.default = pathShim;

export const posix = pathShim;
export const win32 = pathShim;
export default pathShim;
