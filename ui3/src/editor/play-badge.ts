export function playBadgeLabel(paused: boolean, sceneEmpty: boolean): string {
  const state = paused ? "\u{275A}\u{275A} Paused" : "\u{25CF} Running";
  if (sceneEmpty) {
    return `${state} an empty scene \u{2014} nothing placed yet, add items from Insert. Edits are temporary`;
  }
  return `${state} \u{2014} edits are temporary`;
}
