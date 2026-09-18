import { expect, test } from "vitest";
import { runEditorEffect } from "./serial-effect";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

test("drains an active effect before checking and starting its replacement", async () => {
  const first = deferred<void>();
  const calls: string[] = [];
  const active = runEditorEffect("realm-test", () => true, async () => {
    calls.push("first");
    await first.promise;
  });
  const replacement = runEditorEffect("realm-test", () => true, () => {
    calls.push("replacement");
  });

  expect(calls).toEqual([]);
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(calls).toEqual(["first"]);
  first.resolve();
  await active;
  await replacement;
  expect(calls).toEqual(["first", "replacement"]);
});

test("rejects an effect invalidated while it waited in the drain", async () => {
  const first = deferred<void>();
  const active = runEditorEffect("inactive-test", () => true, () => first.promise);
  const stale = runEditorEffect("inactive-test", () => false, () => {
    throw new Error("must not start");
  });
  first.resolve();
  await active;
  await expect(stale).rejects.toMatchObject({ name: "AbortError" });
});
