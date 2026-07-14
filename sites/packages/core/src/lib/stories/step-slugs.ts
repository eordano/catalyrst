export function makeStepSlugs<T extends Record<string, string>>(
  stateToSlug: T,
  initial: keyof T & string,
) {
  type StateId = keyof T & string;
  type StepSlug = T[StateId];

  const firstStepSlug = stateToSlug[initial] as StepSlug;

  const slugToState = Object.fromEntries(
    Object.entries(stateToSlug).map(([state, slug]) => [slug, state]),
  ) as Record<StepSlug, StateId>;

  function toSlug(value: string): StepSlug {
    return stateToSlug[value as StateId] ?? firstStepSlug;
  }

  function toState(slug: string | null | undefined): StateId {
    if (!slug) return initial as StateId;
    return slugToState[slug as StepSlug] ?? (initial as StateId);
  }

  return { stateToSlug, slugToState, firstStepSlug, toSlug, toState };
}
