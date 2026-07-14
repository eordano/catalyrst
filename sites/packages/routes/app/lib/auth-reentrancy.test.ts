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
  it("drops a second call made before the first has finished", async () => {
    const latch: ReentryLatch = { current: false };
    const gate = deferred();
    let runs = 0;
    const run = async () => {
      runs += 1;
      await gate.promise;
    };

    const first = runExclusive(latch, run);
    await runExclusive(latch, run);
    expect(runs).toBe(1);

    gate.resolve();
    await first;
    expect(runs).toBe(1);
  });

  it("lets a genuine retry run once the first call has finished", async () => {
    const latch: ReentryLatch = { current: false };
    let runs = 0;
    const run = async () => {
      runs += 1;
    };

    await runExclusive(latch, run);
    await runExclusive(latch, run);
    expect(runs).toBe(2);
    expect(latch.current).toBe(false);
  });

  it("releases the latch when the call throws, and passes the failure on", async () => {
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
  });

  it("holds the latch for as long as the call is in flight", async () => {
    const latch: ReentryLatch = { current: false };
    const gate = deferred();
    const first = runExclusive(latch, () => gate.promise);
    expect(latch.current).toBe(true);
    gate.resolve();
    await first;
    expect(latch.current).toBe(false);
  });

  it("keeps one latch from blocking another", async () => {
    const approving: ReentryLatch = { current: false };
    const signingIn: ReentryLatch = { current: false };
    const gate = deferred();
    let signedIn = false;

    const first = runExclusive(approving, () => gate.promise);
    await runExclusive(signingIn, async () => {
      signedIn = true;
    });
    expect(signedIn).toBe(true);

    gate.resolve();
    await first;
  });
});
