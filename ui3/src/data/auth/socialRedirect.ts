
import { createIdentityWith } from "./identity";
import { loginWithIdentity } from "./engineLogin";
import { makeInAppSigner, parseAuthResult } from "./thirdweb";

export const AUTH_RESULT_PARAM = "authResult";

export async function completeSocialRedirectLogin(): Promise<boolean> {
  if (typeof window === "undefined") return false;
  let url: URL;
  try {
    url = new URL(window.location.href);
  } catch {
    return false;
  }
  const raw = url.searchParams.get(AUTH_RESULT_PARAM);
  if (!raw) return false;
  url.searchParams.delete(AUTH_RESULT_PARAM);
  try {
    window.history.replaceState(null, "", url.toString());
  } catch {
  }
  const auth = parseAuthResult(raw);
  if (!auth) return false;
  try {
    const signer = makeInAppSigner(auth);
    const identity = await createIdentityWith(signer.address, signer.personalSign);
    return loginWithIdentity(identity);
  } catch {
    return false;
  }
}
