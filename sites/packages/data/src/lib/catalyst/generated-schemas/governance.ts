// GENERATED from catalyrst/ui3/src/generated/catalyst/governance by catalyrst/sites/scripts/gen-zod-schemas.mts. Do not edit.
import { z } from "zod";

import type { ActivityFeedItem } from "@ui/generated/catalyst/governance/ActivityFeedItem";
import type { ActivityPayload } from "@ui/generated/catalyst/governance/ActivityPayload";
import type { ArchiveStatus } from "@ui/generated/catalyst/governance/ArchiveStatus";
import type { BudgetCategory } from "@ui/generated/catalyst/governance/BudgetCategory";
import type { BudgetRow } from "@ui/generated/catalyst/governance/BudgetRow";
import type { BudgetsEnvelope } from "@ui/generated/catalyst/governance/BudgetsEnvelope";
import type { CommentItem } from "@ui/generated/catalyst/governance/CommentItem";
import type { CommentsPayload } from "@ui/generated/catalyst/governance/CommentsPayload";
import type { CreatedProposalBody } from "@ui/generated/catalyst/governance/CreatedProposalBody";
import type { EngagementPayload } from "@ui/generated/catalyst/governance/EngagementPayload";
import type { ErrorBody } from "@ui/generated/catalyst/governance/ErrorBody";
import type { ListEnvelope } from "@ui/generated/catalyst/governance/ListEnvelope";
import type { MemberRow } from "@ui/generated/catalyst/governance/MemberRow";
import type { MembersEnvelope } from "@ui/generated/catalyst/governance/MembersEnvelope";
import type { ProjectConfiguration } from "@ui/generated/catalyst/governance/ProjectConfiguration";
import type { ProjectFunding } from "@ui/generated/catalyst/governance/ProjectFunding";
import type { ProjectLatestUpdate } from "@ui/generated/catalyst/governance/ProjectLatestUpdate";
import type { ProjectOneTimePayment } from "@ui/generated/catalyst/governance/ProjectOneTimePayment";
import type { ProjectRow } from "@ui/generated/catalyst/governance/ProjectRow";
import type { ProjectsEnvelope } from "@ui/generated/catalyst/governance/ProjectsEnvelope";
import type { ProjectUpdateSummary } from "@ui/generated/catalyst/governance/ProjectUpdateSummary";
import type { ProjectVesting } from "@ui/generated/catalyst/governance/ProjectVesting";
import type { ProposalConfiguration } from "@ui/generated/catalyst/governance/ProposalConfiguration";
import type { ProposalRow } from "@ui/generated/catalyst/governance/ProposalRow";
import type { ProposalsEnvelope } from "@ui/generated/catalyst/governance/ProposalsEnvelope";
import type { ProposalVoteItem } from "@ui/generated/catalyst/governance/ProposalVoteItem";
import type { ProposalVotesPayload } from "@ui/generated/catalyst/governance/ProposalVotesPayload";
import type { TopVoterItem } from "@ui/generated/catalyst/governance/TopVoterItem";
import type { VestingLogItem } from "@ui/generated/catalyst/governance/VestingLogItem";
import type { VestingRow } from "@ui/generated/catalyst/governance/VestingRow";
import type { VestingsEnvelope } from "@ui/generated/catalyst/governance/VestingsEnvelope";
import type { VpSeriesPayload } from "@ui/generated/catalyst/governance/VpSeriesPayload";
import type { WeeklyBucket } from "@ui/generated/catalyst/governance/WeeklyBucket";

export const ActivityFeedItemSchema = z.object({
  kind: z.string(),
  address: z.string().nullable(),
  title: z.string().nullable(),
  proposal_id: z.string().nullable(),
  ts: z.number(),
});

export const ActivityPayloadSchema = z.object({
  items: z.array(ActivityFeedItemSchema),
});

export const ArchiveStatusSchema = z.enum(["ok", "archive_unavailable", "not_linked", "not_indexed"]);

export const BudgetCategorySchema = z.object({
  total: z.number(),
  allocated: z.number(),
  available: z.number(),
}).passthrough();

export const BudgetRowSchema = z.object({
  id: z.string(),
  start_at: z.string(),
  finish_at: z.string(),
  total: z.number(),
  allocated: z.number(),
  categories: z.record(z.string(), BudgetCategorySchema),
}).passthrough();

export const BudgetsEnvelopeSchema = z.object({
  data: z.array(BudgetRowSchema),
  limit: z.number(),
  offset: z.number(),
});

export const CommentItemSchema = z.object({
  username: z.string(),
  created_at: z.string(),
  text: z.string(),
});

export const CommentsPayloadSchema = z.object({
  total: z.number(),
  comments: z.array(CommentItemSchema),
  archive_status: ArchiveStatusSchema,
});

export const CreatedProposalBodySchema = z.object({
  id: z.string(),
  type: z.string(),
  snapshot_space: z.string(),
  ipfs: z.string(),
  title: z.string(),
  pending: z.boolean(),
  published: z.boolean(),
});

export const TopVoterItemSchema = z.object({
  address: z.string(),
  votes: z.number(),
  vp: z.number(),
});

export const WeeklyBucketSchema = z.object({
  week_start: z.string(),
  votes: z.number(),
});

export const EngagementPayloadSchema = z.object({
  voters: z.array(TopVoterItemSchema),
  weekly: z.array(WeeklyBucketSchema),
});

export const ErrorBodySchema = z.object({
  error: z.string(),
});

export const ListEnvelopeSchema = z.object({
  data: z.array(z.record(z.string(), z.unknown())),
  limit: z.number(),
  offset: z.number(),
});

export const MemberRowSchema = z.object({
  address: z.string(),
  role: z.string(),
  fetched_at: z.string(),
});

export const MembersEnvelopeSchema = z.object({
  data: z.array(MemberRowSchema),
  limit: z.number(),
  offset: z.number(),
});

export const ProjectConfigurationSchema = z.object({
  category: z.string(),
  size: z.number(),
  tier: z.string().optional(),
  beneficiary: z.string().optional(),
}).passthrough();

export const ProjectOneTimePaymentSchema = z.object({
  enacting_tx: z.string(),
  token: z.string().optional(),
  tx_amount: z.number().optional(),
}).passthrough();

export const ProjectVestingSchema = z.object({
  start_at: z.string(),
  finish_at: z.string(),
  token: z.string(),
  status: z.string(),
  total: z.number(),
  vested: z.number(),
  released: z.number(),
  releasable: z.number(),
}).passthrough();

export const ProjectFundingSchema = z.object({
  enacted_at: z.string().optional(),
  vesting: ProjectVestingSchema.optional(),
  one_time_payment: ProjectOneTimePaymentSchema.optional(),
}).passthrough();

export const ProjectUpdateSummarySchema = z.object({
  index: z.number(),
  introduction: z.string().nullable(),
  health: z.string().nullable(),
  completion_date: z.string().nullable(),
}).passthrough();

export const ProjectLatestUpdateSchema = z.object({
  update: ProjectUpdateSummarySchema.optional(),
  update_timestamp: z.number(),
}).passthrough();

export const ProjectRowSchema = z.object({
  id: z.string(),
  proposal_id: z.string(),
  title: z.string(),
  status: z.string(),
  type: z.string(),
  author: z.string(),
  configuration: ProjectConfigurationSchema,
  funding: ProjectFundingSchema,
  latest_update: ProjectLatestUpdateSchema,
  created_at: z.number(),
  updated_at: z.number(),
}).passthrough();

export const ProjectsEnvelopeSchema = z.object({
  data: z.array(ProjectRowSchema),
  limit: z.number(),
  offset: z.number(),
});

export const ProposalConfigurationSchema = z.object({
  description: z.string().optional(),
  abstract: z.string().optional(),
  category: z.string().optional(),
  tier: z.string().optional(),
  size: z.number().optional(),
  beneficiary: z.string().optional(),
  paymentToken: z.string().optional(),
}).passthrough();

export const ProposalRowSchema = z.object({
  id: z.string(),
  title: z.string(),
  type: z.string(),
  status: z.string(),
  user: z.string(),
  start_at: z.string(),
  finish_at: z.string(),
  created_at: z.string(),
  required_to_pass: z.number(),
  snapshot_id: z.string(),
  configuration: ProposalConfigurationSchema,
}).passthrough();

export const ProposalVoteItemSchema = z.object({
  voter: z.string(),
  choice: z.number(),
  vp: z.number(),
  created_ts: z.number(),
  reason: z.string().nullable(),
});

export const VpSeriesPayloadSchema = z.object({
  ticks: z.array(z.string()),
  choice1: z.array(z.number()),
  choice2: z.array(z.number()),
});

export const ProposalVotesPayloadSchema = z.object({
  choices: z.array(z.string()),
  scores: z.array(z.number()),
  scores_total: z.number(),
  votes_count: z.number(),
  votes: z.array(ProposalVoteItemSchema),
  series: VpSeriesPayloadSchema.nullable(),
  archive_status: ArchiveStatusSchema,
});

export const ProposalsEnvelopeSchema = z.object({
  data: z.array(ProposalRowSchema),
  limit: z.number(),
  offset: z.number(),
});

export const VestingLogItemSchema = z.object({
  topic: z.string(),
  timestamp: z.string(),
  amount: z.number().optional(),
}).passthrough();

export const VestingRowSchema = z.object({
  address: z.string(),
  start_at: z.string(),
  finish_at: z.string(),
  released: z.number(),
  releasable: z.number(),
  vested: z.number(),
  total: z.number(),
  status: z.string(),
  token: z.string(),
  cliff: z.string(),
  vestedPerPeriod: z.array(z.number()),
  logs: z.array(VestingLogItemSchema),
}).passthrough();

export const VestingsEnvelopeSchema = z.object({
  data: z.array(VestingRowSchema),
  limit: z.number(),
  offset: z.number(),
});

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

export type _AssertActivityFeedItem = Assert<Mutual<ActivityFeedItem, z.infer<typeof ActivityFeedItemSchema>>>;
export type _AssertActivityPayload = Assert<Mutual<ActivityPayload, z.infer<typeof ActivityPayloadSchema>>>;
export type _AssertArchiveStatus = Assert<Mutual<ArchiveStatus, z.infer<typeof ArchiveStatusSchema>>>;
export type _AssertBudgetCategory = Assert<Mutual<BudgetCategory, z.infer<typeof BudgetCategorySchema>>>;
export type _AssertBudgetRow = Assert<Mutual<BudgetRow, z.infer<typeof BudgetRowSchema>>>;
export type _AssertBudgetsEnvelope = Assert<Mutual<BudgetsEnvelope, z.infer<typeof BudgetsEnvelopeSchema>>>;
export type _AssertCommentItem = Assert<Mutual<CommentItem, z.infer<typeof CommentItemSchema>>>;
export type _AssertCommentsPayload = Assert<Mutual<CommentsPayload, z.infer<typeof CommentsPayloadSchema>>>;
export type _AssertCreatedProposalBody = Assert<Mutual<CreatedProposalBody, z.infer<typeof CreatedProposalBodySchema>>>;
export type _AssertEngagementPayload = Assert<Mutual<EngagementPayload, z.infer<typeof EngagementPayloadSchema>>>;
export type _AssertErrorBody = Assert<Mutual<ErrorBody, z.infer<typeof ErrorBodySchema>>>;
export type _AssertListEnvelope = Assert<Mutual<ListEnvelope, z.infer<typeof ListEnvelopeSchema>>>;
export type _AssertMemberRow = Assert<Mutual<MemberRow, z.infer<typeof MemberRowSchema>>>;
export type _AssertMembersEnvelope = Assert<Mutual<MembersEnvelope, z.infer<typeof MembersEnvelopeSchema>>>;
export type _AssertProjectConfiguration = Assert<Mutual<ProjectConfiguration, z.infer<typeof ProjectConfigurationSchema>>>;
export type _AssertProjectFunding = Assert<Mutual<ProjectFunding, z.infer<typeof ProjectFundingSchema>>>;
export type _AssertProjectLatestUpdate = Assert<Mutual<ProjectLatestUpdate, z.infer<typeof ProjectLatestUpdateSchema>>>;
export type _AssertProjectOneTimePayment = Assert<Mutual<ProjectOneTimePayment, z.infer<typeof ProjectOneTimePaymentSchema>>>;
export type _AssertProjectRow = Assert<Mutual<ProjectRow, z.infer<typeof ProjectRowSchema>>>;
export type _AssertProjectsEnvelope = Assert<Mutual<ProjectsEnvelope, z.infer<typeof ProjectsEnvelopeSchema>>>;
export type _AssertProjectUpdateSummary = Assert<Mutual<ProjectUpdateSummary, z.infer<typeof ProjectUpdateSummarySchema>>>;
export type _AssertProjectVesting = Assert<Mutual<ProjectVesting, z.infer<typeof ProjectVestingSchema>>>;
export type _AssertProposalConfiguration = Assert<Mutual<ProposalConfiguration, z.infer<typeof ProposalConfigurationSchema>>>;
export type _AssertProposalRow = Assert<Mutual<ProposalRow, z.infer<typeof ProposalRowSchema>>>;
export type _AssertProposalsEnvelope = Assert<Mutual<ProposalsEnvelope, z.infer<typeof ProposalsEnvelopeSchema>>>;
export type _AssertProposalVoteItem = Assert<Mutual<ProposalVoteItem, z.infer<typeof ProposalVoteItemSchema>>>;
export type _AssertProposalVotesPayload = Assert<Mutual<ProposalVotesPayload, z.infer<typeof ProposalVotesPayloadSchema>>>;
export type _AssertTopVoterItem = Assert<Mutual<TopVoterItem, z.infer<typeof TopVoterItemSchema>>>;
export type _AssertVestingLogItem = Assert<Mutual<VestingLogItem, z.infer<typeof VestingLogItemSchema>>>;
export type _AssertVestingRow = Assert<Mutual<VestingRow, z.infer<typeof VestingRowSchema>>>;
export type _AssertVestingsEnvelope = Assert<Mutual<VestingsEnvelope, z.infer<typeof VestingsEnvelopeSchema>>>;
export type _AssertVpSeriesPayload = Assert<Mutual<VpSeriesPayload, z.infer<typeof VpSeriesPayloadSchema>>>;
export type _AssertWeeklyBucket = Assert<Mutual<WeeklyBucket, z.infer<typeof WeeklyBucketSchema>>>;
