const tails = new Map<string, Promise<void>>();

function abortError(): Error {
  const error = new Error("The editor effect is no longer active.");
  error.name = "AbortError";
  return error;
}

export function runEditorEffect<T>(
  key: string,
  active: () => boolean,
  effect: () => Promise<T> | T,
): Promise<T> {
  const previous = tails.get(key) ?? Promise.resolve();
  const next = previous
    .catch(() => undefined)
    .then(() => {
      if (!active()) throw abortError();
      return effect();
    });
  const tail = next.then(() => undefined, () => undefined);
  tails.set(key, tail);
  void tail.then(() => { if (tails.get(key) === tail) tails.delete(key); });
  return next;
}
