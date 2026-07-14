// GENERATED from catalyrst/ui3/src/generated/catalyst/credits by catalyrst/sites/scripts/gen-zod-schemas.mts. Do not edit.
import { z } from "zod";

import type { BalanceOut } from "@ui/generated/catalyst/credits/BalanceOut";
import type { CartLineOut } from "@ui/generated/catalyst/credits/CartLineOut";
import type { CartOut } from "@ui/generated/catalyst/credits/CartOut";
import type { CheckoutOut } from "@ui/generated/catalyst/credits/CheckoutOut";
import type { CheckoutStartOut } from "@ui/generated/catalyst/credits/CheckoutStartOut";
import type { ClaimCreditsResponse } from "@ui/generated/catalyst/credits/ClaimCreditsResponse";
import type { CreditsData } from "@ui/generated/catalyst/credits/CreditsData";
import type { CreditsProgramProgressResponse } from "@ui/generated/catalyst/credits/CreditsProgramProgressResponse";
import type { CurrentSeasonInfo } from "@ui/generated/catalyst/credits/CurrentSeasonInfo";
import type { GoalData } from "@ui/generated/catalyst/credits/GoalData";
import type { GoalProgressData } from "@ui/generated/catalyst/credits/GoalProgressData";
import type { ItemQuoteOut } from "@ui/generated/catalyst/credits/ItemQuoteOut";
import type { ManaTopupOut } from "@ui/generated/catalyst/credits/ManaTopupOut";
import type { ManaTopupQuoteOut } from "@ui/generated/catalyst/credits/ManaTopupQuoteOut";
import type { MockPurchaseOut } from "@ui/generated/catalyst/credits/MockPurchaseOut";
import type { MockTopupOut } from "@ui/generated/catalyst/credits/MockTopupOut";
import type { PackIntentOut } from "@ui/generated/catalyst/credits/PackIntentOut";
import type { PackOut } from "@ui/generated/catalyst/credits/PackOut";
import type { PriceQuotesOut } from "@ui/generated/catalyst/credits/PriceQuotesOut";
import type { PurchaseIntentIn } from "@ui/generated/catalyst/credits/PurchaseIntentIn";
import type { SeasonData } from "@ui/generated/catalyst/credits/SeasonData";
import type { SeasonsData } from "@ui/generated/catalyst/credits/SeasonsData";
import type { UserData } from "@ui/generated/catalyst/credits/UserData";
import type { Week } from "@ui/generated/catalyst/credits/Week";

export const BalanceOutSchema = z.object({
  address: z.string(),
  available: z.string(),
});

export const CartLineOutSchema = z.object({
  itemId: z.string(),
  collection: z.string(),
  urn: z.string(),
  category: z.string(),
  qty: z.number(),
  unitPriceCredits: z.string(),
});

export const CartOutSchema = z.object({
  address: z.string(),
  items: z.array(CartLineOutSchema),
  totalCredits: z.string(),
});

export const CheckoutOutSchema = z.object({
  id: z.number(),
  address: z.string(),
  totalCredits: z.string(),
  status: z.string(),
});

export const CheckoutStartOutSchema = z.object({
  id: z.number(),
  status: z.string(),
  replayed: z.boolean(),
});

export const ClaimCreditsResponseSchema = z.object({
  ok: z.boolean(),
  credits_granted: z.number(),
  isBlockedForClaiming: z.boolean(),
});

export const CreditsDataSchema = z.object({
  available: z.number(),
  earned: z.number(),
  paid: z.number(),
  expiresIn: z.number(),
  isBlockedForClaiming: z.boolean(),
});

export const GoalProgressDataSchema = z.object({
  totalSteps: z.number(),
  completedSteps: z.number(),
});

export const GoalDataSchema = z.object({
  title: z.string(),
  description: z.string(),
  thumbnail: z.string(),
  progress: GoalProgressDataSchema,
  reward: z.number(),
  isClaimed: z.boolean(),
});

export const UserDataSchema = z.object({
  hasStartedProgram: z.boolean(),
});

export const CreditsProgramProgressResponseSchema = z.object({
  user: UserDataSchema,
  credits: CreditsDataSchema,
  goals: z.array(GoalDataSchema),
});

export const SeasonDataSchema = z.object({
  id: z.number(),
  name: z.string(),
  startDate: z.string(),
  endDate: z.string(),
  maxMana: z.string(),
  timeLeft: z.number(),
  amountOfWeeks: z.number(),
  state: z.string(),
});

export const WeekSchema = z.object({
  weekNumber: z.number(),
  timeLeft: z.number(),
  startDate: z.string(),
  endDate: z.string(),
  secondsRemaining: z.number(),
});

export const CurrentSeasonInfoSchema = z.object({
  season: SeasonDataSchema,
  week: WeekSchema,
});

export const ItemQuoteOutSchema = z.object({
  itemId: z.string(),
  collection: z.string(),
  credits: z.string().nullable(),
});

export const ManaTopupOutSchema = z.object({
  creditsGranted: z.string(),
  available: z.string(),
  txHash: z.string(),
});

export const ManaTopupQuoteOutSchema = z.object({
  credits: z.string(),
  weiSuggested: z.string(),
  manaUsd: z.string(),
});

export const MockPurchaseOutSchema = z.object({
  creditsGranted: z.string(),
  available: z.string(),
  mock: z.boolean(),
});

export const MockTopupOutSchema = z.object({
  creditsGranted: z.string(),
  available: z.string(),
  mock: z.boolean(),
});

export const PackIntentOutSchema = z.object({
  clientSecret: z.string(),
  paymentIntentId: z.string(),
});

export const PackOutSchema = z.object({
  sku: z.string(),
  title: z.string(),
  credits: z.string(),
  priceCents: z.number(),
  currency: z.string(),
  sortOrder: z.number(),
});

export const PriceQuotesOutSchema = z.object({
  items: z.array(ItemQuoteOutSchema),
  amounts: z.array(z.string().nullable()),
});

export const PurchaseIntentInSchema = z.object({
  buyer: z.string(),
  items: z.string(),
  totalCredits: z.string(),
  currency: z.string(),
  nonce: z.string(),
  expiresAt: z.number(),
});

export const SeasonsDataSchema = z.object({
  lastSeason: SeasonDataSchema,
  currentSeason: CurrentSeasonInfoSchema,
  nextSeason: SeasonDataSchema,
});

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

export type _AssertBalanceOut = Assert<Mutual<BalanceOut, z.infer<typeof BalanceOutSchema>>>;
export type _AssertCartLineOut = Assert<Mutual<CartLineOut, z.infer<typeof CartLineOutSchema>>>;
export type _AssertCartOut = Assert<Mutual<CartOut, z.infer<typeof CartOutSchema>>>;
export type _AssertCheckoutOut = Assert<Mutual<CheckoutOut, z.infer<typeof CheckoutOutSchema>>>;
export type _AssertCheckoutStartOut = Assert<Mutual<CheckoutStartOut, z.infer<typeof CheckoutStartOutSchema>>>;
export type _AssertClaimCreditsResponse = Assert<Mutual<ClaimCreditsResponse, z.infer<typeof ClaimCreditsResponseSchema>>>;
export type _AssertCreditsData = Assert<Mutual<CreditsData, z.infer<typeof CreditsDataSchema>>>;
export type _AssertCreditsProgramProgressResponse = Assert<Mutual<CreditsProgramProgressResponse, z.infer<typeof CreditsProgramProgressResponseSchema>>>;
export type _AssertCurrentSeasonInfo = Assert<Mutual<CurrentSeasonInfo, z.infer<typeof CurrentSeasonInfoSchema>>>;
export type _AssertGoalData = Assert<Mutual<GoalData, z.infer<typeof GoalDataSchema>>>;
export type _AssertGoalProgressData = Assert<Mutual<GoalProgressData, z.infer<typeof GoalProgressDataSchema>>>;
export type _AssertItemQuoteOut = Assert<Mutual<ItemQuoteOut, z.infer<typeof ItemQuoteOutSchema>>>;
export type _AssertManaTopupOut = Assert<Mutual<ManaTopupOut, z.infer<typeof ManaTopupOutSchema>>>;
export type _AssertManaTopupQuoteOut = Assert<Mutual<ManaTopupQuoteOut, z.infer<typeof ManaTopupQuoteOutSchema>>>;
export type _AssertMockPurchaseOut = Assert<Mutual<MockPurchaseOut, z.infer<typeof MockPurchaseOutSchema>>>;
export type _AssertMockTopupOut = Assert<Mutual<MockTopupOut, z.infer<typeof MockTopupOutSchema>>>;
export type _AssertPackIntentOut = Assert<Mutual<PackIntentOut, z.infer<typeof PackIntentOutSchema>>>;
export type _AssertPackOut = Assert<Mutual<PackOut, z.infer<typeof PackOutSchema>>>;
export type _AssertPriceQuotesOut = Assert<Mutual<PriceQuotesOut, z.infer<typeof PriceQuotesOutSchema>>>;
export type _AssertPurchaseIntentIn = Assert<Mutual<PurchaseIntentIn, z.infer<typeof PurchaseIntentInSchema>>>;
export type _AssertSeasonData = Assert<Mutual<SeasonData, z.infer<typeof SeasonDataSchema>>>;
export type _AssertSeasonsData = Assert<Mutual<SeasonsData, z.infer<typeof SeasonsDataSchema>>>;
export type _AssertUserData = Assert<Mutual<UserData, z.infer<typeof UserDataSchema>>>;
export type _AssertWeek = Assert<Mutual<Week, z.infer<typeof WeekSchema>>>;
