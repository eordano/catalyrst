import { describe, it, expect } from "vitest";
import { renderToString } from "react-dom/server";

import {
  readLease,
  isInReturnWindow,
} from "@data/lib/catalyst/marketplace/escrow-lease";
import LeaseGatedActions from "./LeaseGatedActions";

const NOW = 1_700_000_000_000;
const IN_WINDOW = NOW + 10 * 86_400_000;
const UNLOCKED = NOW - 1;

describe("escrow-lease return-window logic (Phase 5/7 gate)", () => {
  it("is locked while inside the window, free once unlocked, null when not leased", () => {
    expect(isInReturnWindow(readLease({ status: "leased", unlockAt: IN_WINDOW }), NOW)).toBe(true);
    expect(isInReturnWindow(readLease({ status: "leased", unlockAt: UNLOCKED }), NOW)).toBe(false);
    expect(readLease({ status: null, unlockAt: null })).toBeNull();
    expect(
      isInReturnWindow(readLease({ lease: { status: "leased", unlockAt: IN_WINDOW } }), NOW),
    ).toBe(true);
  });
});

describe("LeaseGatedActions (Phase 7 \u{2014} disable Sell/Transfer/List in the UI)", () => {
  it("renders Sell/Transfer/List as disabled buttons (no live links) inside the return window, and live <a> actions for a non-leased or unlocked item", () => {
    const locked = renderToString(
      <LeaseGatedActions item={{ status: "leased", unlockAt: IN_WINDOW }} now={NOW} />,
    );
    expect(locked).toContain('data-locked="true"');
    expect(locked).not.toContain("<a ");
    expect((locked.match(/disabled=""/g) ?? []).length).toBeGreaterThanOrEqual(3);
    expect(locked).toMatch(/Sell/);
    expect(locked).toMatch(/Transfer/);
    expect(locked).toMatch(/return window/i);

    const live = renderToString(
      <LeaseGatedActions
        item={{ status: null, unlockAt: null }}
        actions={[{ key: "sell", label: "Sell", href: "/marketplace/sell" }]}
        now={NOW}
      />,
    );
    expect(live).toContain('data-locked="false"');
    expect(live).toContain("<a ");
    expect(live).not.toContain('disabled=""');

    const unlocked = renderToString(
      <LeaseGatedActions item={{ status: "leased", unlockAt: UNLOCKED }} now={NOW} />,
    );
    expect(unlocked).toContain('data-locked="false"');
    expect(unlocked).not.toContain('disabled=""');
  });
});
