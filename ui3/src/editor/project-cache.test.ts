import { expect, test } from "vitest";
import { createProjectPlayStateWriter } from "./project-cache";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

test("drains a stale play-state true before the reset false write", async () => {
  const first = deferred();
  const writes: boolean[] = [];
  const setPlayState = createProjectPlayStateWriter(async (playing) => {
    writes.push(playing);
    if (playing) await first.promise;
  }, "play-state-test");

  const playing = setPlayState(true);
  const reset = setPlayState(false);
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(writes).toEqual([true]);
  first.resolve();
  await playing;
  await reset;
  expect(writes).toEqual([true, false]);
});
