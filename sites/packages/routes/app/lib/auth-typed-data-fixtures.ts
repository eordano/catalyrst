// Lifted from auth/src/shared/auth/__fixtures__/decentralandTypedData.ts: the generic (non
// MetaTransaction) typed data Decentraland's own dApps ask a wallet to sign. Three are golden
// vectors of exactly what ethers v5 puts on the wire for the domain, types and values each dApp
// passes it -- derived EIP712Domain, declared keys only, integers as decimal strings, lowercased
// addresses, hexlified bytes -- and the builder's cheque is the JSON the builder assembles by hand.
// Imported by the tests that pin what this page accepts and what it shows.

export type TypedDataFixtureField = { name: string; type: string };

export type DappTypedData = {
  primaryType: string;
  types: Record<string, TypedDataFixtureField[]>;
  domain: Record<string, unknown>;
  message: Record<string, unknown>;
};

export const DAPP_USER = "0x1234567890abcdef1234567890abcdef12345678";

const ZERO_ADDRESS = "0x0000000000000000000000000000000000000000";
const COLLECTION_TOKEN_ID =
  "210624583337114373395836055367340864637790190801098222508621955073";
const LAND_TOKEN_ID = "3402823669209384634633746074317682114580";
const COLLECTION = "0x3c2b9b4bd4f8f9a1c0d2e3f4a5b6c7d8e9f0a1b2";

// decentraland-dapps getTradeSignature: the Polygon marketplace domain (salt bytes32(137)) with
// OFFCHAIN_MARKETPLACE_TYPES for a wearable listed for 1 MANA, one ownership check, and the
// beneficiary as the wallet reported it.
const OFFCHAIN_MARKETPLACE_TRADE: DappTypedData = {
  types: {
    Trade: [
      { name: "checks", type: "Checks" },
      { name: "sent", type: "AssetWithoutBeneficiary[]" },
      { name: "received", type: "Asset[]" },
    ],
    Asset: [
      { name: "assetType", type: "uint256" },
      { name: "contractAddress", type: "address" },
      { name: "value", type: "uint256" },
      { name: "extra", type: "bytes" },
      { name: "beneficiary", type: "address" },
    ],
    AssetWithoutBeneficiary: [
      { name: "assetType", type: "uint256" },
      { name: "contractAddress", type: "address" },
      { name: "value", type: "uint256" },
      { name: "extra", type: "bytes" },
    ],
    Checks: [
      { name: "uses", type: "uint256" },
      { name: "expiration", type: "uint256" },
      { name: "effective", type: "uint256" },
      { name: "salt", type: "bytes32" },
      { name: "contractSignatureIndex", type: "uint256" },
      { name: "signerSignatureIndex", type: "uint256" },
      { name: "allowedRoot", type: "bytes32" },
      { name: "externalChecks", type: "ExternalCheck[]" },
    ],
    ExternalCheck: [
      { name: "contractAddress", type: "address" },
      { name: "selector", type: "bytes4" },
      { name: "value", type: "bytes" },
      { name: "required", type: "bool" },
    ],
    EIP712Domain: [
      { name: "name", type: "string" },
      { name: "version", type: "string" },
      { name: "verifyingContract", type: "address" },
      { name: "salt", type: "bytes32" },
    ],
  },
  domain: {
    name: "DecentralandMarketplacePolygon",
    version: "1.0.0",
    verifyingContract: "0xa40b1d129b8906888720686f3a01921ddf37716f",
    salt: "0x0000000000000000000000000000000000000000000000000000000000000089",
  },
  primaryType: "Trade",
  message: {
    checks: {
      uses: "1",
      expiration: "1767225600",
      effective: "1764547200",
      salt: `0x${"ab".repeat(32)}`,
      contractSignatureIndex: "0",
      signerSignatureIndex: "0",
      allowedRoot: `0x${"00".repeat(32)}`,
      externalChecks: [
        {
          contractAddress: COLLECTION,
          selector: "0x70a08231",
          value: "0x",
          required: true,
        },
      ],
    },
    sent: [
      {
        assetType: "3",
        contractAddress: COLLECTION,
        value: COLLECTION_TOKEN_ID,
        extra: "0x",
      },
    ],
    received: [
      {
        assetType: "1",
        contractAddress: "0xa1c57f48f0deb89f569dfbe6e2b7f46d33606fd4",
        value: "1000000000000000000",
        extra: "0x",
        beneficiary: DAPP_USER,
      },
    ],
  },
};

// marketplace signListing: the mainnet Rentals domain with its chainId handed over as bytes32(1),
// a LAND listed for two rental periods. ethers wrote the chainId as "1" and declared it uint256.
const RENTALS_LISTING: DappTypedData = {
  types: {
    Listing: [
      { name: "signer", type: "address" },
      { name: "contractAddress", type: "address" },
      { name: "tokenId", type: "uint256" },
      { name: "expiration", type: "uint256" },
      { name: "indexes", type: "uint256[3]" },
      { name: "pricePerDay", type: "uint256[]" },
      { name: "maxDays", type: "uint256[]" },
      { name: "minDays", type: "uint256[]" },
      { name: "target", type: "address" },
    ],
    EIP712Domain: [
      { name: "name", type: "string" },
      { name: "version", type: "string" },
      { name: "chainId", type: "uint256" },
      { name: "verifyingContract", type: "address" },
    ],
  },
  domain: {
    name: "Rentals",
    version: "1",
    chainId: "1",
    verifyingContract: "0x3a1469499d0be105d4f77045ca403a5f6dc2f3f5",
  },
  primaryType: "Listing",
  message: {
    signer: DAPP_USER,
    contractAddress: "0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d",
    tokenId: LAND_TOKEN_ID,
    expiration: "1767225600",
    indexes: ["0", "3", "1"],
    pricePerDay: ["1000000000000000000", "5000000000000000000"],
    maxDays: ["7", "30"],
    minDays: ["1", "7"],
    target: ZERO_ADDRESS,
  },
};

// snapshot.js Client712.vote on an approval proposal: a domain with neither chainId nor verifying
// contract, and the message as the client completes it.
const GOVERNANCE_SNAPSHOT_VOTE: DappTypedData = {
  types: {
    Vote: [
      { name: "from", type: "address" },
      { name: "space", type: "string" },
      { name: "timestamp", type: "uint64" },
      { name: "proposal", type: "bytes32" },
      { name: "choice", type: "uint32[]" },
      { name: "reason", type: "string" },
      { name: "app", type: "string" },
      { name: "metadata", type: "string" },
    ],
    EIP712Domain: [
      { name: "name", type: "string" },
      { name: "version", type: "string" },
    ],
  },
  domain: { name: "snapshot", version: "0.1.4" },
  primaryType: "Vote",
  message: {
    from: DAPP_USER,
    space: "snapshot.dcl.eth",
    timestamp: "1767225600",
    proposal: `0x${"cd".repeat(32)}`,
    choice: ["1", "3"],
    reason: "",
    app: "snapshot",
    metadata: "{}",
  },
};

// builder getPublishItemsSignature: JSON.stringify of this object, with a numeric qty and a
// checksummed verifying contract.
const BUILDER_CONSUME_SLOTS: DappTypedData = {
  domain: {
    name: "Decentraland Third Party Registry",
    verifyingContract: "0x1C436C1EFb4608dFfDC8bace99d2B03c314f3348",
    version: "1",
    salt: `0x${"00".repeat(31)}89`,
  },
  message: {
    thirdPartyId: "urn:decentraland:matic:collections-thirdparty:cryptohats",
    qty: 10,
    salt: `0x${"5f".repeat(32)}`,
  },
  types: {
    EIP712Domain: [
      { name: "name", type: "string" },
      { name: "version", type: "string" },
      { name: "verifyingContract", type: "address" },
      { name: "salt", type: "bytes32" },
    ],
    ConsumeSlots: [
      { name: "thirdPartyId", type: "string" },
      { name: "qty", type: "uint256" },
      { name: "salt", type: "bytes32" },
    ],
  },
  primaryType: "ConsumeSlots",
};

export const SIGNED_BY_DAPPS: Record<string, DappTypedData> = {
  "off-chain marketplace Trade": OFFCHAIN_MARKETPLACE_TRADE,
  "rentals Listing": RENTALS_LISTING,
  "governance Snapshot vote": GOVERNANCE_SNAPSHOT_VOTE,
  "builder ConsumeSlots cheque": BUILDER_CONSUME_SLOTS,
};
