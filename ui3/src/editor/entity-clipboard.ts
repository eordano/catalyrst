let clipboard: unknown = null;

export async function writeEntityClipboard(value: unknown): Promise<void> {
  clipboard = value;
  try { await navigator.clipboard?.writeText(JSON.stringify({ __dclEntities: value })); } catch { }
}

export async function readEntityClipboard(): Promise<unknown> {
  try {
    const raw = await navigator.clipboard?.readText();
    if (raw) {
      const parsed = JSON.parse(raw) as { __dclEntities?: unknown };
      if (parsed.__dclEntities) return parsed.__dclEntities;
      throw new Error("Copy entities in the hierarchy before pasting.");
    }
  } catch (error) {
    if (error instanceof Error && error.message === "Copy entities in the hierarchy before pasting.") throw error;
  }
  if (!clipboard) throw new Error("Copy entities in the hierarchy before pasting.");
  return clipboard;
}
