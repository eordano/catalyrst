
type ModuleLoader = () => Promise<Record<string, unknown>>;

const modules = import.meta.glob([
  "@ui/**/[A-Z]*.tsx",
  "!**/*.stories.*",
  "!**/*.test.*",
  "!**/*.spec.*",
]) as Record<string, ModuleLoader>;

export type PreviewEntry = { key: string; path: string; load: ModuleLoader };

function shortPath(fullPath: string): string {
  const m = fullPath.match(/ui3\/src\/(.+)\.tsx$/);
  return m ? m[1] : fullPath.replace(/\.tsx$/, "");
}

export function registry(): Map<string, PreviewEntry> {
  const byKey = new Map<string, PreviewEntry>();
  const basenameCount = new Map<string, number>();
  for (const path of Object.keys(modules)) {
    const base = shortPath(path).split("/").pop()!;
    basenameCount.set(base, (basenameCount.get(base) ?? 0) + 1);
  }
  for (const [path, load] of Object.entries(modules)) {
    const short = shortPath(path);
    const base = short.split("/").pop()!;
    byKey.set(short, { key: short, path, load });
    if (basenameCount.get(base) === 1) {
      byKey.set(base, { key: base, path, load });
    }
  }
  return byKey;
}

export function listEntries(): PreviewEntry[] {
  const seen = new Set<string>();
  const out: PreviewEntry[] = [];
  for (const entry of registry().values()) {
    if (seen.has(entry.path)) continue;
    seen.add(entry.path);
    out.push(entry);
  }
  return out.sort((a, b) => a.path.localeCompare(b.path));
}

export async function resolveComponent(
  key: string,
): Promise<{ Component: React.ComponentType<Record<string, unknown>>; path: string } | null> {
  const entry = registry().get(key) ?? registry().get(decodeURIComponent(key));
  if (!entry) return null;
  const mod = await entry.load();
  const candidate =
    (mod.default as React.ComponentType<Record<string, unknown>> | undefined) ??
    (Object.entries(mod).find(
      ([name, value]) => /^[A-Z]/.test(name) && typeof value === "function",
    )?.[1] as React.ComponentType<Record<string, unknown>> | undefined);
  return candidate ? { Component: candidate, path: entry.path } : null;
}
