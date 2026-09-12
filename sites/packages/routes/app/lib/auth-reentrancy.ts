
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
