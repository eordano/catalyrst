export type HierarchyPlacement = "inside" | "before" | "after";

export function hierarchySelection(
  visible: string[], selected: string[], anchor: string | null, id: string,
  modifiers: { shiftKey?: boolean; ctrlKey?: boolean; metaKey?: boolean },
): string[] {
  if (modifiers.shiftKey && anchor && visible.includes(anchor)) {
    const from = visible.indexOf(anchor);
    const to = visible.indexOf(id);
    if (to >= 0) {
      const range = visible.slice(Math.min(from, to), Math.max(from, to) + 1);
      return modifiers.ctrlKey || modifiers.metaKey ? [...new Set([...selected, ...range])] : range;
    }
  }
  if (modifiers.ctrlKey || modifiers.metaKey) return selected.includes(id) ? selected.filter(value => value !== id) : [...selected, id];
  return [id];
}
