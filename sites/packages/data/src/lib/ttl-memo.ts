type TtlMemo<A extends unknown[], T> = ((...args: A) => Promise<T>) & {
  reset: () => void;
};

// Server memo for visitor-independent reads: one upstream call per key per ttl, concurrent
// callers share the in-flight promise; a null key bypasses the memo; rejections and values
// failing `keep` are never retained.
export function ttlMemo<A extends unknown[], T>(opts: {
  ttlMs: number;
  load: (...args: A) => Promise<T>;
  keyOf?: (...args: A) => string | null;
  keep?: (value: T) => boolean;
}): TtlMemo<A, T> {
  const entries = new Map<string, { at: number; value: T }>();
  const inflight = new Map<string, Promise<T>>();
  const memo = ((...args: A): Promise<T> => {
    const key = opts.keyOf ? opts.keyOf(...args) : "";
    if (key === null) return opts.load(...args);
    const hit = entries.get(key);
    if (hit && Date.now() - hit.at < opts.ttlMs) return Promise.resolve(hit.value);
    const pending = inflight.get(key);
    if (pending) return pending;
    const p = opts
      .load(...args)
      .then((value) => {
        if (!opts.keep || opts.keep(value)) entries.set(key, { at: Date.now(), value });
        return value;
      })
      .finally(() => {
        inflight.delete(key);
      });
    inflight.set(key, p);
    return p;
  }) as TtlMemo<A, T>;
  memo.reset = () => {
    entries.clear();
    inflight.clear();
  };
  return memo;
}
