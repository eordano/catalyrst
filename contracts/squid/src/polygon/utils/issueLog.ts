// Minimal structural shape of a subsquid Log needed to pair Issue logs with
// Traded events. Kept dependency-free so it can be unit tested in isolation.
export type IssueLogLike = {
  transactionIndex: number;
  address: string;
  topics: string[];
  logIndex: number;
};

/**
 * Selects the Issue log that corresponds to a given MarketplaceV3 `Traded`
 * event when minting an item as a primary sale.
 *
 * A single transaction can mint the same `itemId` more than once (e.g. buying
 * several units of the same item at once). Each mint emits its own `Issue` log
 * that differs only by `issuedId`/`tokenId`, and the marketplace emits one
 * `Traded` event per unit. Because `Traded` events are processed in log order,
 * we consume the matching `Issue` logs in ascending `logIndex` order and skip
 * any already matched to a previous `Traded` event in the same batch. Using
 * `Array.find` on its own would return the first matching `Issue` log every
 * time and silently drop every mint after the first for that item.
 *
 * The chosen log is recorded in `consumedIssueLogs` (keyed by
 * `${blockHeight}-${logIndex}`) so the next `Traded` event for the same item
 * picks the following issuance.
 */
export function selectIssueLogForTrade<T extends IssueLogLike>(
  logs: T[],
  params: {
    transactionIndex: number;
    collectionAddress: string;
    itemId: bigint;
    issueTopic: string;
    blockHeight: number;
    consumedIssueLogs: Set<string>;
  }
): T | undefined {
  const {
    transactionIndex,
    collectionAddress,
    itemId,
    issueTopic,
    blockHeight,
    consumedIssueLogs,
  } = params;

  const matchingIssueLogs = logs
    .filter(
      (l) =>
        l.transactionIndex === transactionIndex &&
        l.address.toLowerCase() === collectionAddress.toLowerCase() &&
        l.topics.length >= 4 &&
        l.topics[0] === issueTopic &&
        BigInt(l.topics[3]) === itemId
    )
    .sort((a, b) => a.logIndex - b.logIndex);

  const chosen = matchingIssueLogs.find(
    (l) => !consumedIssueLogs.has(`${blockHeight}-${l.logIndex}`)
  );

  if (chosen) {
    consumedIssueLogs.add(`${blockHeight}-${chosen.logIndex}`);
  }

  return chosen;
}
