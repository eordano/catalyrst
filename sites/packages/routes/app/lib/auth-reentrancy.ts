// Mirrors auth/src/components/Pages/RequestPage/RequestPage.tsx's isApprovingRef: the buttons that
// dispatch to the wallet disable themselves from React state, which does not settle synchronously,
// so a double click re-enters the handler before the first pass has changed the phase. A ref does
// settle, and releasing it on every exit path leaves a genuine retry after a failure free to run.

export type ReentryLatch = { current: boolean };

export async function runExclusive(
  latch: ReentryLatch,
  run: () => Promise<void>,
): Promise<void> {
  if (latch.current) return;
  latch.current = true;
  try {
    await run();
  } finally {
    latch.current = false;
  }
}
