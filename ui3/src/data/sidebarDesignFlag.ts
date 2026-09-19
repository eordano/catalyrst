import { flagState } from "./featureFlags";

export const SIDEBAR_DESIGN_FLAG = "2026-09-sidebar-design";
export const SIDEBAR_DESIGN_STORAGE = `dcl.feature.${SIDEBAR_DESIGN_FLAG}`;

export function sidebarDesignEnabled(): boolean { return flagState(SIDEBAR_DESIGN_FLAG).enabled; }
