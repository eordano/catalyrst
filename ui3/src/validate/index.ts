// The single entry point every boundary imports from.
//
// It re-exports from the bare id `dcl-validate-impl`, which resolves to
// `checked.ts` by default and to `unchecked.ts` when the build sets
// performance mode. A bare id rather than a relative path because that is what
// both Vite's `resolve.alias` and tsconfig `paths` can redirect; a relative
// `./impl` cannot be aliased without matching every `./impl` in the tree.
//
// Why an alias at all, rather than `if (ENABLED)`: a runtime branch keeps zod
// and all 215 schema definitions in the bundle and constructs them at module
// load, which is most of the cost the mode is meant to remove. Swapping the
// module lets the bundler drop the import edge entirely, and
// `scripts/check-perf-strip.mts` asserts it actually did -- a perf build that
// silently kept zod would be the worst outcome, paying the bytes while
// believing it had not.

export {
  check,
  checkOk,
  VALIDATION_ENABLED,
  validationFailures,
  resetValidationFailures,
  setValidationReporter,
  setValidationDevMode,
} from "dcl-validate-impl";
export type { ValidationReporter } from "dcl-validate-impl";
