import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { randomBytes } from "node:crypto";
import { dirname, join } from "node:path";

import {
  type ControlResult,
  available,
  unavailable,
} from "../catalyst/admin/availability";

/**
 * File-backed operator env vars, in systemd EnvironmentFile syntax so the
 * exact file this page edits is the one units load at start. Lines this store
 * does not own (comments, hand-added entries with unusual syntax) are
 * preserved verbatim; only `NAME=` lines it manages are rewritten.
 */

export type OperatorEnvEntry = { name: string; value: string };

export type OperatorEnvFile = {
  path: string;
  entries: OperatorEnvEntry[];
  preservedLines: number;
};

export const ENV_NAME_RE = /^[A-Z][A-Z0-9_]{0,63}$/;
const VALUE_MAX = 4096;

export function envFilePath(): string | undefined {
  const p = process.env.CATALYRST_OPERATOR_ENV_FILE;
  return p && p.trim() !== "" ? p.trim() : undefined;
}

const NOT_CONFIGURED = () =>
  unavailable(
    "not-configured",
    "Operator env persistence is not wired: this process has no CATALYRST_OPERATOR_ENV_FILE.",
    {
      serverCheck: "catalyrst/nixos/sites.nix",
      fix: "set CATALYRST_OPERATOR_ENV_FILE to a path the sites process can write (the nixos module wires /var/lib/catalyrst-sites/operator.env)",
    },
  );

function parseLine(line: string): OperatorEnvEntry | null {
  const m = /^([A-Z][A-Z0-9_]*)=(.*)$/.exec(line);
  return m ? { name: m[1], value: m[2] } : null;
}

async function readLines(path: string): Promise<string[] | null> {
  try {
    const raw = await readFile(path, "utf8");
    return raw.split("\n").filter((l, i, all) => !(l === "" && i === all.length - 1));
  } catch (e) {
    if ((e as { code?: string })?.code === "ENOENT") return null;
    throw e;
  }
}

export async function readOperatorEnv(): Promise<ControlResult<OperatorEnvFile>> {
  const path = envFilePath();
  if (!path) return NOT_CONFIGURED();
  let lines: string[] | null;
  try {
    lines = await readLines(path);
  } catch (e) {
    return unavailable("backend-error", `Operator env file is unreadable: ${(e as Error).message}`, {
      serverCheck: "catalyrst/nixos/sites.nix",
    });
  }
  const entries: OperatorEnvEntry[] = [];
  let preserved = 0;
  for (const line of lines ?? []) {
    const entry = parseLine(line);
    if (entry) entries.push(entry);
    else if (line.trim() !== "") preserved += 1;
  }
  return available({ path, entries, preservedLines: preserved });
}

function validate(name: string, value: string): string | null {
  if (!ENV_NAME_RE.test(name)) {
    return "names are UPPER_SNAKE_CASE: start with A-Z, then A-Z 0-9 _, at most 64 chars";
  }
  if (/[\n\r]/.test(value)) return "values are a single line";
  if (value.length > VALUE_MAX) return `values are at most ${VALUE_MAX} chars`;
  return null;
}

async function writeAtomically(path: string, lines: string[]): Promise<void> {
  await mkdir(dirname(path), { recursive: true });
  const tmp = join(dirname(path), `.operator-env.${randomBytes(6).toString("hex")}.tmp`);
  const body = lines.length === 0 ? "" : lines.join("\n") + "\n";
  await writeFile(tmp, body, { mode: 0o600 });
  await rename(tmp, path);
}

async function mutate(
  fn: (lines: string[]) => string[],
): Promise<ControlResult<OperatorEnvFile>> {
  const path = envFilePath();
  if (!path) return NOT_CONFIGURED();
  try {
    const lines = (await readLines(path)) ?? [];
    await writeAtomically(path, fn(lines));
  } catch (e) {
    return unavailable("backend-error", `Operator env file write failed: ${(e as Error).message}`, {
      serverCheck: "catalyrst/nixos/sites.nix",
      fix: "make the file's directory writable by the sites unit (StateDirectory=catalyrst-sites)",
    });
  }
  return readOperatorEnv();
}

export async function upsertOperatorEnv(
  name: string,
  value: string,
): Promise<ControlResult<OperatorEnvFile>> {
  const invalid = validate(name, value);
  if (invalid) return unavailable("misrouted", `Rejected ${name || "(empty)"}: ${invalid}`, { serverCheck: null });
  return mutate((lines) => {
    const next = `${name}=${value}`;
    const at = lines.findIndex((l) => parseLine(l)?.name === name);
    if (at >= 0) {
      const copy = [...lines];
      copy[at] = next;
      return copy;
    }
    return [...lines, next];
  });
}

export async function removeOperatorEnv(name: string): Promise<ControlResult<OperatorEnvFile>> {
  if (!ENV_NAME_RE.test(name)) {
    return unavailable("misrouted", `Rejected ${name || "(empty)"}: not an env var name`, { serverCheck: null });
  }
  return mutate((lines) => lines.filter((l) => parseLine(l)?.name !== name));
}
