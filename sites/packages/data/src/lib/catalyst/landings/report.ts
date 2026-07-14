import { z } from "zod";

import { catalystBase, postJSON } from "../client";
import { signedFetch } from "../../auth/signer";
import type { AuthIdentity } from "../../auth/types";

export const REPORT_REASONS = [
  "scam_phishing",
  "illegal_content",
  "harassment",
  "cheating",
  "impersonation",
] as const;
export type ReportReason = (typeof REPORT_REASONS)[number];

export const REPORT_LIMITS = {
  descriptionMax: 500,
  commentsMax: 500,
  maxFiles: 5,
  maxFileSizeMb: 10,
} as const;

export const ReasonOptionSchema = z.object({
  value: z.string().min(1),
  label: z.string().min(1),
});
export type ReasonOption = z.infer<typeof ReasonOptionSchema>;

export const EvidenceFileSchema = z.object({
  id: z.string().min(1),
  name: z.string().min(1),
  size: z.number().nonnegative(),
});
export type EvidenceFile = z.infer<typeof EvidenceFileSchema>;

export type EvidenceUpload = EvidenceFile & {
  blob?: Blob;
  contentType?: string;
};

export const ReportCopySchema = z
  .object({
    reportedLabel: z.string(),
    reportedHelper: z.string(),
    reportedPlaceholder: z.string(),
    reasonLabel: z.string(),
    descriptionLabel: z.string(),
    descriptionPlaceholder: z.string(),
    descriptionHint: z.string(),
    evidenceLabel: z.string(),
    evidenceHelper: z.string(),
    additionalLabel: z.string(),
    additionalHelper: z.string(),
    additionalPlaceholder: z.string(),
    confirmationLabel: z.string(),
    submit: z.string(),
    submitFailed: z.string(),
    yourWalletHelper: z.string(),
    signInAlert: z.string(),
    walletMismatch: z.string(),
    errors: z.object({
      invalidAddress: z.string(),
      missingReason: z.string(),
      missingDescription: z.string(),
      missingEvidence: z.string(),
      mustConfirm: z.string(),
    }),
  })
  .partial();

/**
 * The copy fixture for the report wizard. Every key is required, so a
 * partial fixture file fails to parse rather than handing the page a
 * complete-looking title/message/reason-list nobody actually supplied.
 * `loadFixture` in landings.report-abuse.tsx turns a failed parse into
 * `null`, and the wizard falls back to its own built-in copy -- so a partial
 * file reads as no file rather than as this one.
 */
export const ReportFixtureSchema = z.object({
  title: z.string(),
  successTitle: z.string(),
  successBody: z.string(),
  successDismiss: z.string(),
  limits: z
    .object({
      descriptionMax: z.number(),
      commentsMax: z.number(),
      maxFiles: z.number(),
      maxFileSizeMb: z.number(),
      acceptedFileTypes: z.array(z.string()),
    })
    .partial(),
  reasonOptions: z.array(ReasonOptionSchema),
  copy: ReportCopySchema,
  sampleReporter: z.object({ playerAddress: z.string() }).partial(),
  sampleTarget: z
    .object({
      reportedAddress: z.string(),
      reason: z.string(),
      description: z.string(),
      evidence: z.array(EvidenceFileSchema),
      additionalComments: z.string(),
    })
    .partial(),
});
export type ReportFixture = z.infer<typeof ReportFixtureSchema>;

export function parseReportFixture(raw: unknown): ReportFixture {
  return ReportFixtureSchema.parse(raw);
}

export type ReportDraft = {
  playerAddress: string;
  reportedAddress: string;
  reason: ReportReason | "";
  description: string;
  evidence: EvidenceUpload[];
  additionalComments: string;
  confirmAccuracy: boolean;
};

export function isEthAddress(v: string | undefined | null): boolean {
  return /^0x[a-fA-F0-9]{40}$/.test((v ?? "").trim());
}

export type ReportFieldErrors = {
  reportedAddress: string;
  reason: string;
  description: string;
  evidence: string;
  confirmAccuracy: string;
};

const E = {
  invalidAddress: "Please enter a valid wallet address",
  missingReason: "Please choose a reason",
  missingDescription: "Please include a description of your report",
  missingEvidence: "Please upload the evidence of your issue",
  mustConfirm: "You must confirm this information is accurate",
} as const;

export function validateTarget(reportedAddress: string): string {
  return isEthAddress(reportedAddress) ? "" : E.invalidAddress;
}

export function validateDraft(draft: ReportDraft): ReportFieldErrors {
  return {
    reportedAddress: validateTarget(draft.reportedAddress),
    reason: draft.reason ? "" : E.missingReason,
    description: draft.description.trim() ? "" : E.missingDescription,
    evidence: draft.evidence.length === 0 ? E.missingEvidence : "",
    confirmAccuracy: draft.confirmAccuracy ? "" : E.mustConfirm,
  };
}

export function invalidFields(errors: ReportFieldErrors): string[] {
  return Object.entries(errors)
    .filter(([, msg]) => msg !== "")
    .map(([field]) => field);
}

export type SubmitReportResult = {
  reportId: string;
  evidenceKeys: string[];
};

export type SubmitReportFn = (args: {
  draft: ReportDraft;
  signal?: AbortSignal;
}) => Promise<SubmitReportResult>;

export const failClosedSubmitReport: SubmitReportFn = async () => {
  throw new Error("report submission unavailable: report service not configured");
};

export const COMMS_PREFIX = "/comms";
export const REPORT_PRESIGN_PATH = "/reports/players/presign";
export const REPORT_CREATE_PATH = "/reports/players";

export const NOT_CONNECTED_MESSAGE = "Connect your wallet to submit a report.";
export const MISSING_BYTES_MESSAGE =
  "Re-attach your evidence files before submitting.";

const PresignSlotSchema = z.object({
  key: z.string().min(1),
  uploadPath: z.string().min(1),
});

const PresignResponseSchema = z.object({
  reportId: z.string().min(1),
  files: z.array(PresignSlotSchema),
});

/** `evidenceKeys` is what the service says it stored. Defaulting to `[]` would
 *  tell a reporter their evidence was filed when the response never said so --
 *  the one claim on this screen they cannot check. Required: a create response
 *  without it is not a create response. */
const CreatedReportSchema = z.object({
  data: z.object({
    id: z.string().min(1),
    evidenceKeys: z.array(z.string()),
  }),
});

function contentTypeOf(file: EvidenceUpload): string {
  return file.contentType || file.blob?.type || "";
}

export function buildSubmitReport(
  identity: AuthIdentity | null,
): SubmitReportFn {
  return async ({ draft, signal }): Promise<SubmitReportResult> => {
    if (!identity) throw new Error(NOT_CONNECTED_MESSAGE);

    const errors = validateDraft(draft);
    const invalid = invalidFields(errors);
    if (invalid.length > 0) {
      throw new Error(errors[invalid[0] as keyof ReportFieldErrors]);
    }

    const uploads = draft.evidence.map((file) => {
      const blob = file.blob;
      const contentType = contentTypeOf(file);
      if (!blob || !contentType) throw new Error(MISSING_BYTES_MESSAGE);
      return { name: file.name, blob, contentType };
    });

    const presignRaw = await postJSON<unknown>(
      `${COMMS_PREFIX}${REPORT_PRESIGN_PATH}`,
      {
        files: uploads.map((u) => ({
          filename: u.name,
          contentType: u.contentType,
          fileSize: u.blob.size,
        })),
      },
      { identity, signPath: REPORT_PRESIGN_PATH, signal },
    );
    const presign = PresignResponseSchema.safeParse(presignRaw);
    if (!presign.success || presign.data.files.length !== uploads.length) {
      throw new Error("Report service returned an unexpected upload plan.");
    }

    const base = catalystBase();
    for (const [index, slot] of presign.data.files.entries()) {
      const upload = uploads[index];
      const res = await signedFetch(
        identity,
        `${base}${COMMS_PREFIX}${slot.uploadPath}`,
        {
          method: "PUT",
          signPath: slot.uploadPath,
          metadata: {},
          headers: { "content-type": upload.contentType },
          body: upload.blob,
          signal,
        },
      );
      if (!res.ok) {
        throw new Error(
          `Evidence upload failed for ${upload.name} (${res.status}).`,
        );
      }
    }

    const comments = draft.additionalComments.trim();
    const createdRaw = await postJSON<unknown>(
      `${COMMS_PREFIX}${REPORT_CREATE_PATH}`,
      {
        reportId: presign.data.reportId,
        reportedAddress: draft.reportedAddress.trim().toLowerCase(),
        reason: draft.reason,
        description: draft.description.trim(),
        additionalComments: comments || undefined,
        confirmAccuracy: draft.confirmAccuracy,
        evidenceKeys: presign.data.files.map((f) => f.key),
      },
      { identity, signPath: REPORT_CREATE_PATH, signal },
    );
    const created = CreatedReportSchema.safeParse(createdRaw);
    if (!created.success) {
      throw new Error("Report service returned an unexpected response.");
    }

    return {
      reportId: created.data.data.id,
      evidenceKeys: created.data.data.evidenceKeys,
    };
  };
}
