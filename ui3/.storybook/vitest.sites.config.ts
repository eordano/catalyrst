import { configDefaults, mergeConfig } from "vitest/config";

import base from "../vitest.browser.config.ts";
// @ts-expect-error -- plain JS helper, shared with the vite configs so the
// validation seam resolves the same way under test as it does in a build.
import { validateAlias } from "../vite.validate.js";

// vitest.browser.config.ts plus the validation-seam alias, which sites story
// import graphs reach (via @data -> @ui/validate) and the base config omits.
// Run from ui3/ so the base config's cwd-relative paths keep resolving:
//   npx vitest run --config .storybook/vitest.sites.config.ts <file filters>
export default mergeConfig(base, {
  resolve: { alias: validateAlias() },
  test: {
    exclude: [...configDefaults.exclude],
  },
});
