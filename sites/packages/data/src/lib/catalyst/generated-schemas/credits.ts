// GENERATED from catalyrst/ui3/src/generated/catalyst/credits by catalyrst/sites/scripts/gen-zod-schemas.mts. Do not edit.
import { z } from "zod";

import type { AuthorizeCreditOut } from "@ui/generated/catalyst/credits/AuthorizeCreditOut";
import type { AuthorizedCreditOut } from "@ui/generated/catalyst/credits/AuthorizedCreditOut";
import type { BalanceOut } from "@ui/generated/catalyst/credits/BalanceOut";
import type { CartLineOut } from "@ui/generated/catalyst/credits/CartLineOut";
import type { CartOut } from "@ui/generated/catalyst/credits/CartOut";
import type { CheckoutOut } from "@ui/generated/catalyst/credits/CheckoutOut";
import type { CheckoutSessionOut } from "@ui/generated/catalyst/credits/CheckoutSessionOut";
import type { CheckoutStartOut } from "@ui/generated/catalyst/credits/CheckoutStartOut";
import type { ClaimCreditsResponse } from "@ui/generated/catalyst/credits/ClaimCreditsResponse";
import type { CreditsData } from "@ui/generated/catalyst/credits/CreditsData";
import type { CreditsOrderStatusOut } from "@ui/generated/catalyst/credits/CreditsOrderStatusOut";
import type { CreditsProgramProgressResponse } from "@ui/generated/catalyst/credits/CreditsProgramProgressResponse";
import type { CreditsTotals } from "@ui/generated/catalyst/credits/CreditsTotals";
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
import type { ReleaseIntentsOut } from "@ui/generated/catalyst/credits/ReleaseIntentsOut";
import type { SeasonData } from "@ui/generated/catalyst/credits/SeasonData";
import type { SeasonsData } from "@ui/generated/catalyst/credits/SeasonsData";
import type { UnityCreditPack } from "@ui/generated/catalyst/credits/UnityCreditPack";
import type { UnityCreditPacksResponse } from "@ui/generated/catalyst/credits/UnityCreditPacksResponse";
import type { UsdCredits } from "@ui/generated/catalyst/credits/UsdCredits";
import type { UserCreditItem } from "@ui/generated/catalyst/credits/UserCreditItem";
import type { UserCreditsResponse } from "@ui/generated/catalyst/credits/UserCreditsResponse";
import type { UserData } from "@ui/generated/catalyst/credits/UserData";
import type { Week } from "@ui/generated/catalyst/credits/Week";

export const AuthorizedCreditOutSchema = z.object({
  id: z.string(),
  amount: z.string(),
  availableAmount: z.string(),
  expiresAt: z.number(),
  signature: z.string(),
  contract: z.string(),
});

export const AuthorizeCreditOutSchema = z.object({
  credit: AuthorizedCreditOutSchema,
  maxCreditedValue: z.string(),
  usdCents: z.number(),
  oracleRate: z.string(),
});

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

export const CheckoutSessionOutSchema = z.object({
  orderId: z.string(),
  url: z.string(),
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

export const CreditsOrderStatusOutSchema = z.object({
  status: z.string(),
  creditsGranted: z.number(),
  newBalance: z.number(),
  error: z.string(),
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

export const CreditsTotalsSchema = z.object({
  expiring: z.number(),
  nonExpiring: z.number(),
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

export const ReleaseIntentsOutSchema = z.object({
  ok: z.boolean(),
});

export const SeasonsDataSchema = z.object({
  lastSeason: SeasonDataSchema,
  currentSeason: CurrentSeasonInfoSchema,
  nextSeason: SeasonDataSchema,
});

export const UnityCreditPackSchema = z.object({
  id: z.string(),
  usd: z.number(),
  credits: z.number(),
  recommended: z.boolean(),
  order: z.number(),
});

export const UnityCreditPacksResponseSchema = z.object({
  packs: z.array(UnityCreditPackSchema),
});

export const UsdCreditsSchema = z.object({
  balanceCents: z.coerce.bigint(),
  credits: z.number(),
});

export const UserCreditItemSchema = z.object({
  id: z.string(),
  userAddress: z.string(),
  amount: z.string(),
  availableAmount: z.string(),
  status: z.string(),
  contract: z.string(),
  timestamp: z.string(),
  signature: z.string(),
  seasonId: z.number(),
  goalId: z.string(),
  weekId: z.number(),
  claimedAt: z.string(),
  expiresAt: z.string(),
  creditSource: z.string(),
});

export const UserCreditsResponseSchema = z.object({
  credits: z.array(UserCreditItemSchema),
  totalCredits: z.number(),
  totals: CreditsTotalsSchema,
  usd: UsdCreditsSchema,
});

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

export type _AssertAuthorizeCreditOut = Assert<Mutual<AuthorizeCreditOut, z.infer<typeof AuthorizeCreditOutSchema>>>;
export type _AssertAuthorizedCreditOut = Assert<Mutual<AuthorizedCreditOut, z.infer<typeof AuthorizedCreditOutSchema>>>;
export type _AssertBalanceOut = Assert<Mutual<BalanceOut, z.infer<typeof BalanceOutSchema>>>;
export type _AssertCartLineOut = Assert<Mutual<CartLineOut, z.infer<typeof CartLineOutSchema>>>;
export type _AssertCartOut = Assert<Mutual<CartOut, z.infer<typeof CartOutSchema>>>;
export type _AssertCheckoutOut = Assert<Mutual<CheckoutOut, z.infer<typeof CheckoutOutSchema>>>;
export type _AssertCheckoutSessionOut = Assert<Mutual<CheckoutSessionOut, z.infer<typeof CheckoutSessionOutSchema>>>;
export type _AssertCheckoutStartOut = Assert<Mutual<CheckoutStartOut, z.infer<typeof CheckoutStartOutSchema>>>;
export type _AssertClaimCreditsResponse = Assert<Mutual<ClaimCreditsResponse, z.infer<typeof ClaimCreditsResponseSchema>>>;
export type _AssertCreditsData = Assert<Mutual<CreditsData, z.infer<typeof CreditsDataSchema>>>;
export type _AssertCreditsOrderStatusOut = Assert<Mutual<CreditsOrderStatusOut, z.infer<typeof CreditsOrderStatusOutSchema>>>;
export type _AssertCreditsProgramProgressResponse = Assert<Mutual<CreditsProgramProgressResponse, z.infer<typeof CreditsProgramProgressResponseSchema>>>;
export type _AssertCreditsTotals = Assert<Mutual<CreditsTotals, z.infer<typeof CreditsTotalsSchema>>>;
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
export type _AssertReleaseIntentsOut = Assert<Mutual<ReleaseIntentsOut, z.infer<typeof ReleaseIntentsOutSchema>>>;
export type _AssertSeasonData = Assert<Mutual<SeasonData, z.infer<typeof SeasonDataSchema>>>;
export type _AssertSeasonsData = Assert<Mutual<SeasonsData, z.infer<typeof SeasonsDataSchema>>>;
export type _AssertUnityCreditPack = Assert<Mutual<UnityCreditPack, z.infer<typeof UnityCreditPackSchema>>>;
export type _AssertUnityCreditPacksResponse = Assert<Mutual<UnityCreditPacksResponse, z.infer<typeof UnityCreditPacksResponseSchema>>>;
export type _AssertUsdCredits = Assert<Mutual<UsdCredits, z.infer<typeof UsdCreditsSchema>>>;
export type _AssertUserCreditItem = Assert<Mutual<UserCreditItem, z.infer<typeof UserCreditItemSchema>>>;
export type _AssertUserCreditsResponse = Assert<Mutual<UserCreditsResponse, z.infer<typeof UserCreditsResponseSchema>>>;
export type _AssertUserData = Assert<Mutual<UserData, z.infer<typeof UserDataSchema>>>;
export type _AssertWeek = Assert<Mutual<Week, z.infer<typeof WeekSchema>>>;
