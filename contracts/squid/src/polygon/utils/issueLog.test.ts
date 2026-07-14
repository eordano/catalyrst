import assert from "node:assert";
import { describe, it } from "node:test";
import { IssueLogLike, selectIssueLogForTrade } from "./issueLog";

const ISSUE_TOPIC =
  "0x57e2fe3f7dcd918a54e57b2dc0da8e347386daa9d69c3bbf6c8bce2f7e8398c7";
const COLLECTION = "0x03b1940d80394614a5ba60abbf73fa749068bdad";
const OTHER_TOPIC =
  "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

// itemId is the indexed 4th topic of the Issue event (padded to 32 bytes).
const topicForItem = (itemId: number) =>
  "0x" + itemId.toString(16).padStart(64, "0");

const issueLog = (
  logIndex: number,
  itemId: number,
  overrides: Partial<IssueLogLike> = {}
): IssueLogLike => ({
  transactionIndex: 0,
  address: COLLECTION,
  topics: [ISSUE_TOPIC, "0x", "0x", topicForItem(itemId)],
  logIndex,
  ...overrides,
});

describe("selectIssueLogForTrade", () => {
  const baseParams = () => ({
    transactionIndex: 0,
    collectionAddress: COLLECTION,
    issueTopic: ISSUE_TOPIC,
    blockHeight: 42899045,
    consumedIssueLogs: new Set<string>(),
  });

  it("when a tx mints the same item twice it returns a distinct Issue log per call", () => {
    // Mirrors tx 0xa49ba5...: item 5 issued twice, item 15 once, all via MarketplaceV3.
    const logs: IssueLogLike[] = [
      issueLog(3, 5), // item 5, issued #1
      issueLog(10, 5), // item 5, issued #2
      issueLog(17, 15), // item 15, issued #1
    ];
    const consumed = new Set<string>();
    const params = { ...baseParams(), consumedIssueLogs: consumed };

    const first = selectIssueLogForTrade(logs, { ...params, itemId: 5n });
    const second = selectIssueLogForTrade(logs, { ...params, itemId: 5n });
    const third = selectIssueLogForTrade(logs, { ...params, itemId: 15n });

    assert.ok(first && second && third, "all three mints must resolve a log");
    assert.strictEqual(first.logIndex, 3, "first item-5 Traded -> issued #1");
    assert.strictEqual(
      second.logIndex,
      10,
      "second item-5 Traded -> issued #2 (previously dropped)"
    );
    assert.strictEqual(third.logIndex, 17, "item-15 Traded -> its own log");
    assert.notStrictEqual(
      first.logIndex,
      second.logIndex,
      "the two item-5 mints must map to different Issue logs"
    );
  });

  it("when there is a single mint it returns that log", () => {
    const logs = [issueLog(3, 5)];
    const chosen = selectIssueLogForTrade(logs, {
      ...baseParams(),
      itemId: 5n,
    });
    assert.strictEqual(chosen?.logIndex, 3);
  });

  it("when there are no more unconsumed logs it returns undefined", () => {
    const logs = [issueLog(3, 5)];
    const params = { ...baseParams(), itemId: 5n };
    assert.ok(selectIssueLogForTrade(logs, params));
    assert.strictEqual(
      selectIssueLogForTrade(logs, params),
      undefined,
      "the single Issue log is consumed and not reused"
    );
  });

  it("when logs are out of order it consumes them in ascending logIndex order", () => {
    const logs = [issueLog(10, 5), issueLog(3, 5)];
    const params = { ...baseParams(), itemId: 5n };
    const first = selectIssueLogForTrade(logs, params);
    const second = selectIssueLogForTrade(logs, params);
    assert.strictEqual(first?.logIndex, 3);
    assert.strictEqual(second?.logIndex, 10);
  });

  it("when logs belong to another item, tx or contract they are ignored", () => {
    const logs: IssueLogLike[] = [
      issueLog(3, 15), // different item
      issueLog(4, 5, { transactionIndex: 9 }), // different tx
      issueLog(5, 5, { address: "0xdeadbeef" }), // different contract
      issueLog(6, 5, { topics: [OTHER_TOPIC, "0x", "0x", topicForItem(5)] }), // not an Issue
      issueLog(7, 5), // the only valid match
    ];
    const chosen = selectIssueLogForTrade(logs, {
      ...baseParams(),
      itemId: 5n,
    });
    assert.strictEqual(chosen?.logIndex, 7);
  });
});
