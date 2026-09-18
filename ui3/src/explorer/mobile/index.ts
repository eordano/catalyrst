import { lazy } from "react";

export { useIsMobile } from "./detect";

export { WORLD_CANVAS_ID, isSynthesizedWorldKey } from "./keys";

export * from "./layout";

export const TouchControls = lazy(() =>
  import("./controls").then((m) => ({ default: m.TouchControls })),
);

export const MobileHudFrame = lazy(() =>
  import("./chrome").then((m) => ({ default: m.MobileHudFrame })),
);
