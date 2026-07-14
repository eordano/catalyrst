import { MemoryRouter } from "react-router";

import type { Decorator, Meta, StoryObj } from "@ui/docs/sb";
import { expect, userEvent, waitFor } from "@ui/docs/sb";

import {
  REPORT_REASONS,
  RESOLUTION_OPTIONS,
  decisionToStatus,
  toReportCard,
  type ReportRow,
} from "@data/lib/catalyst/admin/places-moderation";
import type { ModerateFn } from "./machine";
import ModeratePlacesWizard from "./ModeratePlacesWizard";

const report = (over: Partial<ReportRow>): ReportRow => ({
  id: "0",
  entity_id: "bafkreiplacexyz",
  reporter: "0x89aB3c00112233445566778899aabbccddeeff00",
  status: "open",
  reason: "scam_or_spam",
  resolution: null,
  notes: null,
  resolved_by: null,
  resolved_at: null,
  created_at: "2026-07-01T12:00:00Z",
  place_title: "Neon Bazaar",
  place_coords: "12,-34",
  place_image: null,
  place_creator: "0x1111222233334444555566667777888899990000",
  payload: null,
  ...over,
});

const reports: ReportRow[] = [
  report({ id: "101" }),
  report({
    id: "102",
    place_title: "Moon Casino",
    place_coords: "3,7",
    reason: "adult_content",
    reporter: "0x22aa000000000000000000000000000000000022",
  }),
  report({
    id: "103",
    status: "resolved",
    resolution: "no_violation",
    resolved_by: "0x33bb000000000000000000000000000000000033",
    resolved_at: "2026-07-02T08:00:00Z",
    place_title: "Calm Garden",
    place_coords: "-8,15",
  }),
  report({ id: "104", status: "dismissed", place_title: "Sky Docks", place_coords: "40,2" }),
];

const moderate: ModerateFn = async ({ report: r, decision, resolution, notes, disablePlace }) => ({
  report: { ...r, status: decisionToStatus(decision), resolution: resolution ?? null, notes: notes ?? null },
  placeDisabled: Boolean(disablePlace),
  reportBody: { status: decisionToStatus(decision), resolution, notes },
});

const withRouter: Decorator = (Story) => (
  <MemoryRouter>
    <Story />
  </MemoryRouter>
);

const meta = {
  title: "Sites Specs/admin/places-moderation/ModeratePlacesWizard",
  component: ModeratePlacesWizard,
  decorators: [withRouter],
  parameters: {
    layout: "padded",
    a11y: { test: "todo" },
  },
  args: {
    trackCtx: { sid: "sb-spec", story: "admin/places-moderation", variant: "bucketed_queue" },
    reports,
    cards: reports.map(toReportCard),
    reasons: REPORT_REASONS,
    resolutions: RESOLUTION_OPTIONS,
    total: reports.length,
    moderate,
    track: () => {},
  },
} satisfies Meta<typeof ModeratePlacesWizard>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Queue: Story = { args: { initialStep: "queue" } };

export const ReviewReport: Story = { args: { initialStep: "review-report" } };

export const Decision: Story = { args: { initialStep: "decision" } };

export const Moderated: Story = { args: { initialStep: "moderated" } };

// The wizard opens straight into the queue: the "Open moderation console"
// auth-gate click was removed as frontend-authorization theatre (access is
// decided server-side; see the note above STATE_TO_SLUG in machine.ts).
export const HappyPath: Story = {
  play: async ({ canvas, canvasElement }) => {
    const reviews = await canvas.findAllByRole("button", { name: "Review" });
    await userEvent.click(reviews[0]);
    await userEvent.click(await canvas.findByRole("button", { name: "Resolve" }));
    await canvas.findByText(/Decide on report/);
    await canvas.findByRole("button", { name: /Confirm resolve/i });
    await waitFor(() =>
      expect(canvasElement.querySelector('[data-state="decision"]')).toBeTruthy(),
    );
  },
};
