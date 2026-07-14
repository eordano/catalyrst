import fs from "node:fs";
import path from "node:path";

import { z } from "zod";

export const DownloadOptionSchema = z.object({
  os: z.string(),
  osKey: z.enum(["macos", "windows"]),
  arch: z.string(),
  archLabel: z.string(),
  ext: z.string(),
  fileName: z.string(),
  sizeBytes: z.number().nonnegative(),
  url: z.url(),
});
export type DownloadOption = z.infer<typeof DownloadOptionSchema>;

const FixtureSchema = z.object({
  repo: z.string(),
  releasesLatestUrl: z.string().url(),
  version: z.string(),
  publishedAt: z.string(),
  releaseUrl: z.url(),
  options: z.record(z.string(), DownloadOptionSchema),
  defaultOption: z.string(),
  systemRequirements: z.record(z.string(), z.string()).default({}),
});
export type DownloadFixture = z.infer<typeof FixtureSchema>;

const FIXTURE_PATH = path.join(
  process.cwd(),
  "packages",
  "data",
  "src",
  "fixtures",
  "landings-creator-hub-download.json",
);

const FALLBACK: DownloadFixture = {
  repo: "decentraland/creator-hub",
  releasesLatestUrl: "https://github.com/decentraland/creator-hub/releases/latest",
  version: "latest",
  publishedAt: "",
  releaseUrl: "https://github.com/decentraland/creator-hub/releases/latest",
  options: {
    "macos-arm64": {
      os: "macOS",
      osKey: "macos",
      arch: "arm64",
      archLabel: "Apple Silicon",
      ext: "dmg",
      fileName: "Decentraland-Creator-Hub-mac-arm64.dmg",
      url: "https://github.com/decentraland/creator-hub/releases/latest",
      sizeBytes: 0,
    },
    "windows-amd64": {
      os: "Windows",
      osKey: "windows",
      arch: "amd64",
      archLabel: "x64",
      ext: "exe",
      fileName: "Decentraland-Creator-Hub-win-x64.exe",
      url: "https://github.com/decentraland/creator-hub/releases/latest",
      sizeBytes: 0,
    },
  },
  defaultOption: "macos-arm64",
  systemRequirements: {},
};

export function loadDownloadFixture(): DownloadFixture {
  try {
    const raw = fs.readFileSync(FIXTURE_PATH, "utf8");
    const parsed = FixtureSchema.safeParse(JSON.parse(raw));
    if (parsed.success) return parsed.data;
  } catch {
  }
  return FALLBACK;
}

export type OsKey = "macos" | "windows";
export type ArchKey = "arm64" | "amd64";

export function detectOs(userAgent: string | null | undefined): OsKey {
  const ua = (userAgent ?? "").toLowerCase();
  if (/windows|win64|win32|wow64/.test(ua)) return "windows";
  if (/mac os x|macintosh|mac_powerpc|darwin/.test(ua)) return "macos";
  return "macos";
}

export function detectArch(userAgent: string | null | undefined, os: OsKey): ArchKey {
  const ua = (userAgent ?? "").toLowerCase();
  if (/arm64|aarch64/.test(ua)) return "arm64";
  if (os === "windows") return "amd64";
  if (/intel/.test(ua) && !/apple/.test(ua)) return "amd64";
  return os === "macos" ? "arm64" : "amd64";
}

function normalizeOs(v: string | null | undefined): OsKey | undefined {
  const s = (v ?? "").toLowerCase();
  if (s === "windows" || s === "win") return "windows";
  if (s === "macos" || s === "mac" || s === "osx" || s === "darwin") return "macos";
  return undefined;
}

function normalizeArch(v: string | null | undefined): ArchKey | undefined {
  const s = (v ?? "").toLowerCase();
  if (s === "arm64" || s === "aarch64" || s === "apple" || s === "silicon") return "arm64";
  if (s === "amd64" || s === "x64" || s === "x86_64" || s === "intel") return "amd64";
  return undefined;
}

export type ResolvedDownload = {
  version: string;
  releaseUrl: string;
  repo: string;
  overridden: boolean;
  detectedOs: OsKey;
  detectedArch: ArchKey;
  primary: DownloadOption;
  secondary: DownloadOption[];
  systemRequirements: Record<string, string>;
};

function pickKey(
  options: Record<string, DownloadOption>,
  os: OsKey,
  arch: ArchKey,
): string | undefined {
  const exact = `${os}-${arch}`;
  if (options[exact]) return exact;
  const sameOs = Object.keys(options).find((k) => options[k].osKey === os);
  return sameOs;
}

export function resolveDownload(
  fixture: DownloadFixture,
  userAgent: string | null | undefined,
  override?: { os?: string | null; arch?: string | null },
): ResolvedDownload {
  const osOverride = normalizeOs(override?.os);
  const detectedOs = osOverride ?? detectOs(userAgent);
  const archOverride = normalizeArch(override?.arch);
  const detectedArch = archOverride ?? detectArch(userAgent, detectedOs);

  const options = fixture.options;
  const primaryKey =
    pickKey(options, detectedOs, detectedArch) ?? fixture.defaultOption;
  const primary = options[primaryKey] ?? FALLBACK.options["macos-arm64"];

  const secondary: DownloadOption[] = [];
  const seenOs = new Set<OsKey>([primary.osKey]);
  for (const key of Object.keys(options)) {
    const opt = options[key];
    if (seenOs.has(opt.osKey)) continue;
    const altKey = pickKey(options, opt.osKey, detectedArch);
    const chosen = (altKey && options[altKey]) || opt;
    secondary.push(chosen);
    seenOs.add(opt.osKey);
  }

  return {
    version: fixture.version,
    releaseUrl: fixture.releaseUrl,
    repo: fixture.repo,
    overridden: osOverride !== undefined || archOverride !== undefined,
    detectedOs,
    detectedArch,
    primary,
    secondary,
    systemRequirements: fixture.systemRequirements ?? {},
  };
}
