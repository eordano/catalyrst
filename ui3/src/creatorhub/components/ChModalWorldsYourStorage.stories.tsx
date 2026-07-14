// Deliberately NOT collapsed into one Controls story + Catalog. `chrome={false}` now drops the
// Creator Hub frame, which removes the duplicate `<main id="ch-main">` / `<nav>` landmarks, and
// the panel labels itself with a `useId()` id rather than a hardcoded one -- but the panel's
// heading is the fixed string "Your Storage", so N stacked panels are N `role="region"`
// landmarks with the same accessible name and axe's `landmark-unique` fails the Catalog.
// Unblocking this needs the panel's landmark name to become per-instance (an `ariaLabel`
// passthrough, or dropping the region role on the panel variant) -- a component change beyond
// the chrome opt-out.
import ChModalWorldsYourStorage from "./ChModalWorldsYourStorage";

export default {
  title: "CreatorHub/Components/Worlds Your Storage",
  component: ChModalWorldsYourStorage,
  parameters: { layout: "fullscreen" },
  args: { variant: "panel" },
};

const MB = 1024 * 1024;

export const Default = {
  args: {
    open: true,
    stats: {
      usedSpace: String(Math.round(47.52 * MB)),
      maxAllowedSpace: String(300 * MB),
    },
    accountHoldings: {
      ownedLands: 1,
      ownedNames: 1,
      ownedMana: 4321,
    },
  },
};

export const NoHoldings = {
  args: {
    open: true,
    stats: {
      usedSpace: "0",
      maxAllowedSpace: String(100 * MB),
    },
    accountHoldings: {
      ownedLands: 0,
      ownedNames: 0,
      ownedMana: 0,
    },
  },
};

export const HoldingsLoading = {
  args: {
    open: true,
    stats: {
      usedSpace: String(Math.round(12.4 * MB)),
      maxAllowedSpace: String(500 * MB),
    },
    accountHoldings: null,
  },
};

export const LargeQuota = {
  args: {
    open: true,
    stats: {
      usedSpace: String(Math.round(640 * MB)),
      maxAllowedSpace: String(4 * 1024 * MB),
    },
    accountHoldings: {
      ownedLands: 24,
      ownedNames: 11,
      ownedMana: 128450,
    },
  },
};

export const Closed = {
  args: {
    open: false,
  },
};
