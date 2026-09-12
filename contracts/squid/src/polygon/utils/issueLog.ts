export type IssueLogLike = {
  transactionIndex: number;
  address: string;
  topics: string[];
  logIndex: number;
};

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
