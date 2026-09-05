/**
 * Per-instance landmark names.
 *
 * axe's `landmark-unique` compares landmarks by **accessible name**, not by id -- so a Storybook
 * `Catalog` that stacks N copies of a page emits N landmarks all called "Profile sections" and
 * fails the gate. `useId()` does not help: the name comes from a fixed string, not an id.
 *
 * The fix is an optional `labelSuffix` prop on the component that owns the landmarks, threaded
 * through every `aria-label` it renders. It follows the `MarketplaceChromeMaybe` / `chrome`
 * convention: optional, and the default preserves today's behaviour byte-for-byte.
 *
 *   <ProfileTabLayout ... />                            -> nav[aria-label="Profile sections"]
 *   <ProfileTabLayout labelSuffix="(with aside)" ... />  -> nav[aria-label="Profile sections (with aside)"]
 */
export function suffixLabel(base: string, suffix?: string): string {
  return suffix ? `${base} ${suffix}` : base;
}

/** Props mixin for components that thread `labelSuffix` through their landmark names. */
export type LabelSuffixProps = {
  /**
   * Appended to every landmark accessible name this component renders, so several instances can
   * coexist on one page without tripping axe's `landmark-unique`. Omitted (the default) leaves
   * the names exactly as they are.
   */
  labelSuffix?: string;
};
