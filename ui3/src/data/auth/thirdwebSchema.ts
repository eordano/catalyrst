// What thirdweb's API hands back, and what the same-origin route that fronts it
// hands back in turn.
//
// Hand-written, unlike the modules under src/generated: thirdweb is a
// third-party SaaS and no Rust type describes it -- not one `.rs` file in this
// repo mentions it -- so there is nothing for gen-zod-schemas.mts to generate
// from. The asserts at the bottom stand in for a generator, failing typecheck
// the day the declared TypeScript shape and the schema disagree.
//
// One module for BOTH clients. ui3/src/data/auth/thirdweb.ts and
// sites/packages/data/src/lib/auth/thirdweb/{api,signer}.ts are near-identical
// twins of the same upstream, and two hand copies of one shape would drift with
// nothing to notice: check-schema-dupes indexes only the generated schemas, so
// hand-versus-hand duplication is invisible to every gate in this tree. sites
// imports these as `@ui/data/auth/thirdwebSchema`, the path it already uses for
// `@ui/data/auth/signerCore`.
//

// `z.looseObject`, not `z.object`: zod STRIPS unknown keys, and these values are
// round-tripped. A persisted blob is read, modified and written back, so a
// stripping parse permanently deletes any field this build does not know about
// -- a newer build's field, erased by an older one during a partial rollout.
// Verified against pending-store, which reads through `check` and writes the
// result straight back. External responses get the same treatment because they
// are forwarded onward and the upstream is free to add fields.
//
// Transforms and coercion still apply; loose only changes what happens to keys
// the shape does not mention.
import { z } from "zod";

import type { ThirdwebAuthResult } from "./thirdweb";

/**
 * `POST /v1/auth/complete` -- the email/OTP exchange that produces a session.
 *
 * Only `token` and `walletAddress` are consumed, but every field is required
 * because every field is declared: the type this satisfies is what the rest of
 * the app is allowed to believe.
 */
export const ThirdwebAuthResultSchema = z.looseObject({
  isNewUser: z.boolean(),
  token: z.string(),
  userId: z.string(),
  walletAddress: z.string(),
  type: z.string(),
});

/**
 * `POST /v1/wallets/sign-message` and `/v1/wallets/sign-typed-data` -- the
 * enclave signing calls sites makes with the server's secret key.
 *
 * The nesting is the contract: a body that carries a bare `signature` is a
 * different endpoint's answer, and dereferencing `.result.signature` on it
 * yields undefined rather than a signature.
 */
export const EnclaveSignatureSchema = z.looseObject({
  result: z.looseObject({ signature: z.string() }),
});

/**
 * `GET /v1/wallets/me` -- the token-to-address lookup.
 *
 * Both spellings are accepted because the caller already reads both: thirdweb
 * has answered with the address at the root and nested under `result`, and this
 * records that rather than picking a winner it cannot enforce.
 */
export const WalletsMeSchema = z.looseObject({
  result: z.looseObject({ address: z.string().nullish() }).nullish(),
  address: z.string().nullish(),
});

/**
 * `POST /internal/thirdweb-sign` on success -- the same-origin route that keeps
 * the thirdweb secret key off the client.
 *
 * Same-origin, but still this boundary: the body is a proxied enclave answer,
 * and the route is deployed and versioned separately from the page calling it.
 * The failure body (`{error}`) is deliberately not modelled -- see the call
 * sites, which must keep answering a 503 with the server's own explanation.
 */
export const SignProxyOkSchema = z.looseObject({
  signature: z.string(),
});

export type WalletsMe = z.infer<typeof WalletsMeSchema>;

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

export type _AssertThirdwebAuthResult = Assert<
  Mutual<ThirdwebAuthResult, z.infer<typeof ThirdwebAuthResultSchema>>
>;
