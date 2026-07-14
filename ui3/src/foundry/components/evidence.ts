// Evidence lives in operator-side directories — some of them ephemeral scratch
// paths that carry the host layout and a session id. None of that belongs on a
// public page, and the absolute path is not openable by a reader anyway. These
// helpers publish a stable identifier instead: the basename, plus a short hash of
// the full path so two files that share a basename stay distinguishable.

/** The last path segment. An absolute host path is never published whole. */
export function basename(path: string): string {
  const trimmed = path.replace(/[/\\]+$/, "");
  const seg = trimmed.split(/[/\\]/).pop() ?? "";
  return seg || trimmed;
}

function shortHash(input: string): string {
  let h = 5381;
  for (let i = 0; i < input.length; i += 1) {
    h = ((h << 5) + h + input.charCodeAt(i)) >>> 0;
  }
  return h.toString(36).padStart(6, "0").slice(0, 6);
}

/** Basename + a short content hash of the full path. Non-leaking and stable. */
export function evidenceLabel(path: string): string {
  return `${basename(path)} · ${shortHash(path)}`;
}
