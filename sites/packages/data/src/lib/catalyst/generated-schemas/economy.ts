// GENERATED from catalyrst/ui3/src/generated/catalyst/economy by catalyrst/sites/scripts/gen-zod-schemas.mts. Do not edit.
import { z } from "zod";

import type { PaymentsConfig } from "@ui/generated/catalyst/economy/PaymentsConfig";
import type { PaymentsNonceOut } from "@ui/generated/catalyst/economy/PaymentsNonceOut";

export const PaymentsConfigSchema = z.object({
  chainId: z.number(),
  enabled: z.boolean(),
  manaToken: z.string().nullable(),
  payTo: z.string().nullable(),
});

export const PaymentsNonceOutSchema = z.object({
  nonce: z.string(),
});

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

export type _AssertPaymentsConfig = Assert<Mutual<PaymentsConfig, z.infer<typeof PaymentsConfigSchema>>>;
export type _AssertPaymentsNonceOut = Assert<Mutual<PaymentsNonceOut, z.infer<typeof PaymentsNonceOutSchema>>>;
