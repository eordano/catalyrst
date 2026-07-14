export {
  THIRDWEB_API_BASE,
  LOGIN_CHAIN_ID,
  thirdwebClientId,
  hasThirdwebClientId,
} from "./config";

export {
  ThirdwebError,
  initiateEmailLogin,
  completeEmailLogin,
  socialLoginUrl,
  signMessageEnclave,
  signTypedDataEnclave,
  getWalletForToken,
} from "./api";
export type {
  ThirdwebAuthResult,
  ThirdwebSocialProvider,
  Eip712TypedData,
} from "./api";

export { makeInAppSigner, proxySignTypedData } from "./signer";
export type { InAppSigner } from "./signer";

export {
  THIRDWEB_SESSION_KEY,
  getThirdwebSession,
  setThirdwebSession,
  clearThirdwebSession,
} from "./session";
export type { ThirdwebSession } from "./session";
