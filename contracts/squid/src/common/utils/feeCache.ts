export type FeeCache<T extends object> = {
  begin(fromBlock: number): void;
  end(toBlock: number): void;
  view(key: string): T;
  write(key: string, patch: Partial<T>): void;
  reset(): void;
};

export function createFeeCache<T extends object>(
  empty: () => T,
  merge: (base: T, patch: Partial<T>) => T
): FeeCache<T> {
  const committed = new Map<string, T>();
  let staged: Map<string, T> | null = null;
  let pending: {
    fromBlock: number;
    toBlock: number;
    writes: Map<string, T>;
  } | null = null;
  let promotedToBlock = -1;
  let openedAtBlock = -1;

  const promote = (writes: Map<string, T>) => {
    for (const [key, patch] of writes) {
      committed.set(key, merge(committed.get(key) ?? empty(), patch));
    }
  };

  return {
    begin(fromBlock: number) {
      const parked = pending;
      pending = null;
      if (fromBlock <= promotedToBlock) {
        committed.clear();
        promotedToBlock = -1;
      } else if (parked) {
        if (fromBlock > parked.toBlock) {
          promote(parked.writes);
          promotedToBlock = parked.toBlock;
        } else if (fromBlock > parked.fromBlock) {
          for (const key of parked.writes.keys()) {
            committed.delete(key);
          }
        }
      }
      openedAtBlock = fromBlock;
      staged = new Map();
    },

    end(toBlock: number) {
      if (staged) {
        pending = { fromBlock: openedAtBlock, toBlock, writes: staged };
      }
      staged = null;
    },

    view(key: string): T {
      const normalized = key.toLowerCase();
      return merge(
        committed.get(normalized) ?? empty(),
        staged?.get(normalized) ?? empty()
      );
    },

    write(key: string, patch: Partial<T>) {
      const normalized = key.toLowerCase();
      const target = staged ?? committed;
      target.set(normalized, merge(target.get(normalized) ?? empty(), patch));
    },

    reset() {
      committed.clear();
      staged = null;
      pending = null;
      promotedToBlock = -1;
      openedAtBlock = -1;
    },
  };
}
