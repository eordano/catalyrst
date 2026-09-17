import type { AllowedMethod } from "./auth-request-params";

export const MALICIOUS_REQUEST_TITLE = "If this request is malicious, it could:";

const TRUST_THE_ASKER = "Only continue if you trust the scene or app that asked for this.";

const TRANSACTION_WARNINGS = [
  "Move, sell or destroy any tokens, NFTs, LAND or names your wallet holds.",
  "Give another account permission to take your assets later, without asking you again.",
  "Once sent, it can't be undone.",
  TRUST_THE_ASKER,
];

const TYPED_DATA_WARNINGS = [
  "Authorize an order, a listing or a spending permission over your assets.",
  "Let a contract act for you later: a signature doesn't expire, and anyone who holds it can submit it.",
  "Log you in to another site or app as you.",
  TRUST_THE_ASKER,
];

const MESSAGE_WARNINGS = [
  "Log you in to another site or app as you.",
  "Authorize an order, a listing or a spending permission over your assets.",
  TRUST_THE_ASKER,
];

export function requestWarnings(method: AllowedMethod): readonly string[] {
  switch (method) {
    case "eth_sendTransaction":
      return TRANSACTION_WARNINGS;
    case "eth_signTypedData_v3":
    case "eth_signTypedData_v4":
      return TYPED_DATA_WARNINGS;
    case "personal_sign":
      return MESSAGE_WARNINGS;
  }
}
