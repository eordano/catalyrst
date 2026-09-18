import { describe, expect, it } from "vitest";

import { runExclusive, type ReentryLatch } from "./auth-reentrancy";

function deferred(): { promise: Promise<void>; resolve: () => void; reject: (e: Error) => void } {
  let resolve!: () => void;
  let reject!: (e: Error) => void;
  const promise = new Promise<void>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("runExclusive", () => {
  it("holds the latch while a call is in flight, drops re-entrant calls, then lets a genuine retry run", async () => {
    const latch: ReentryLatch = { current: false };
    const gate = deferred();
    let runs = 0;
    const run = async () => {
      runs += 1;
      await gate.promise;
    };

    const first = runExclusive(latch, run);
    expect(latch.current).toBe(true);
    await runExclusive(latch, run);
    expect(runs).toBe(1);

    gate.resolve();
    await first;
    expect(runs).toBe(1);
    expect(latch.current).toBe(false);

    await runExclusive(latch, run);
    expect(runs).toBe(2);
    expect(latch.current).toBe(false);
  });

  it("releases the latch when the call throws, passes the failure on, and keeps one latch from blocking another", async () => {
    const latch: ReentryLatch = { current: false };
    await expect(
      runExclusive(latch, async () => {
        throw new Error("boom");
      }),
    ).rejects.toThrow("boom");
    expect(latch.current).toBe(false);

    let ran = false;
    await runExclusive(latch, async () => {
      ran = true;
    });
    expect(ran).toBe(true);

    const approving: ReentryLatch = { current: false };
    const signingIn: ReentryLatch = { current: false };
    const gate = deferred();
    let signedIn = false;
    const first = runExclusive(approving, () => gate.promise);
    await runExclusive(signingIn, async () => {
      signedIn = true;
    });
    expect(signedIn).toBe(true);
    expect(approving.current).toBe(true);

    gate.resolve();
    await first;
    expect(approving.current).toBe(false);
  });
});
