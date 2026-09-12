
import { z } from "zod";

import type { ThirdwebAuthResult } from "./thirdweb";

export const ThirdwebAuthResultSchema = z.looseObject({
  isNewUser: z.boolean(),
  token: z.string(),
  userId: z.string(),
  walletAddress: z.string(),
  type: z.string(),
});

export const EnclaveSignatureSchema = z.looseObject({
  result: z.looseObject({ signature: z.string() }),
});

export const WalletsMeSchema = z.looseObject({
  result: z.looseObject({ address: z.string().nullish() }).nullish(),
  address: z.string().nullish(),
});

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
