import { createElement } from "react";
import { fireEvent, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import FdAskPage, { type FdAskPageProps } from "./pages/FdAskPage";
import FdRequestCard, {
  type FdRequestCardVM,
} from "./components/FdRequestCard";
import FdSessionsPage, {
  type FdSessionOccurrenceVM,
  type FdSessionsPageProps,
} from "./pages/FdSessionsPage";

// The 2026-08-21 journey gaps, kept fixed at the DOM: an ask's author gets an
// edit affordance and readers see the revision stamp; a session card carries
// its RSVP identifiers on the DOM (not only in the hydration payload) and its
// RSVP buttons a straight-quote accessible name.

const noop = () => undefined;

function askVM(overrides: Partial<FdRequestCardVM> = {}): FdRequestCardVM {
  return {
    id: "rq-1",
    title: "Bring back parkour weekends",
    body: "A body",
    source: "my own ask, made here",
    status: "open",
    pledges: 1,
    pledgedByMe: false,
    origin: "visitor",
    author: { name: "Zap" },
    authoredByMe: false,
    createdAt: "2026-08-20T00:00:00Z",
    editedAt: null,
    ...overrides,
  };
}

function askProps(overrides: Partial<FdAskPageProps> = {}): FdAskPageProps {
  return {
    ask: askVM(),
    reading: null,
    pledgeList: [],
    onPledge: noop,
    onWithdraw: noop,
    backHref: "/foundry/exchange",
    ...overrides,
  };
}

describe("FdAskPage author edit affordance", () => {
  it("shows no edit control to a reader who is not the author", () => {
    const { queryByText } = render(createElement(FdAskPage, askProps()));
    expect(queryByText("Edit your ask")).toBeNull();
  });

  it("opens the editor prefilled and saves the revised fields", () => {
    const onSave = vi.fn();
    const onToggle = vi.fn();
    const { getByText, rerender, container } = render(
      createElement(
        FdAskPage,
        askProps({
          ask: askVM({ authoredByMe: true }),
          edit: { open: false, onToggle, onSave, errors: {} },
        }),
      ),
    );
    fireEvent.click(getByText("Edit your ask"));
    expect(onToggle).toHaveBeenCalled();

    rerender(
      createElement(
        FdAskPage,
        askProps({
          ask: askVM({ authoredByMe: true }),
          edit: { open: true, onToggle, onSave, errors: {} },
        }),
      ),
    );
    const title = container.querySelector<HTMLInputElement>("#fd-ask-edit-title")!;
    const body = container.querySelector<HTMLTextAreaElement>("#fd-ask-edit-body")!;
    expect(title.value).toBe("Bring back parkour weekends");
    expect(body.value).toBe("A body");

    fireEvent.change(title, { target: { value: "Parkour weekends, again" } });
    fireEvent.change(body, { target: { value: "A revised body." } });
    fireEvent.submit(container.querySelector("form.fd-form")!);
    expect(onSave).toHaveBeenCalledWith({
      title: "Parkour weekends, again",
      body: "A revised body.",
    });
  });

  it("an over-limit body fails loudly at the form, saving nothing", () => {
    const onSave = vi.fn();
    const { container, getAllByRole } = render(
      createElement(
        FdAskPage,
        askProps({
          ask: askVM({ authoredByMe: true }),
          edit: { open: true, onToggle: noop, onSave, errors: {} },
        }),
      ),
    );
    const body = container.querySelector<HTMLTextAreaElement>("#fd-ask-edit-body")!;
    fireEvent.change(body, { target: { value: "z".repeat(300) } });
    fireEvent.submit(container.querySelector("form.fd-form")!);
    expect(onSave).not.toHaveBeenCalled();
    const alerts = getAllByRole("alert").map((el) => el.textContent ?? "");
    expect(alerts.some((t) => t.includes("your text is kept"))).toBe(true);
  });
});

describe("FdRequestCard revision stamp", () => {
  it("stamps an edited ask for every reader", () => {
    const { container } = render(
      createElement(FdRequestCard, {
        ...askVM({ editedAt: "2026-08-22T10:00:00Z" }),
        onPledge: noop,
        onWithdraw: noop,
      }),
    );
    expect(container.textContent).toContain("edited Aug 22, 2026");
  });

  it("stamps nothing on a never-edited ask", () => {
    const { container } = render(
      createElement(FdRequestCard, {
        ...askVM(),
        onPledge: noop,
        onWithdraw: noop,
      }),
    );
    expect(container.textContent).not.toContain("edited");
  });
});

function occurrence(
  overrides: Partial<FdSessionOccurrenceVM> = {},
): FdSessionOccurrenceVM {
  return {
    seriesId: "series-1",
    title: "Tuesday relay night",
    body: "",
    sceneId: null,
    sceneTitle: null,
    cadence: "weekly",
    occurrenceAt: "2026-08-25T19:00:00.000Z",
    durationMinutes: 60,
    host: { name: "Zap" },
    rsvpCount: 2,
    viewerRsvped: false,
    label: "weekly",
    ...overrides,
  };
}

function sessionsProps(
  overrides: Partial<FdSessionsPageProps> = {},
): FdSessionsPageProps {
  return {
    occurrences: [],
    canHost: false,
    createOpen: false,
    onToggleCreate: noop,
    onRsvp: noop,
    onWithdraw: noop,
    onCreate: noop,
    onRetire: noop,
    ...overrides,
  };
}

describe("FdSessionsPage RSVP identifiers on the DOM", () => {
  const occurrences = [
    occurrence(),
    occurrence({
      seriesId: "series-2",
      title: "Build lab",
      occurrenceAt: "2026-08-26T18:00:00.000Z",
      viewerRsvped: true,
    }),
  ];

  it("stamps seriesId and occurrenceAt on every card, so a script needs no hydration payload", () => {
    const { container } = render(
      createElement(FdSessionsPage, sessionsProps({ occurrences })),
    );
    for (const o of occurrences) {
      const card = container.querySelector(
        `[data-series-id="${o.seriesId}"][data-occurrence-at="${o.occurrenceAt}"]`,
      );
      expect(card, `${o.seriesId} card missing its data attributes`).not.toBeNull();
    }
  });

  it("names the RSVP and Withdraw buttons per session, straight quotes only", () => {
    const { getByLabelText } = render(
      createElement(FdSessionsPage, sessionsProps({ occurrences })),
    );
    const rsvp = getByLabelText("RSVP: Tuesday relay night");
    const withdraw = getByLabelText("Withdraw RSVP: Build lab");
    expect(rsvp.tagName).toBe("BUTTON");
    expect(withdraw.tagName).toBe("BUTTON");
    // The visible label's curly apostrophe defeated text-matching automation;
    // the accessible name must never carry one.
    expect(rsvp.getAttribute("aria-label")).not.toMatch(/’/);
  });

  it("the stamped identifiers are exactly what the RSVP submits", () => {
    const onRsvp = vi.fn();
    const { container, getByLabelText } = render(
      createElement(FdSessionsPage, sessionsProps({ occurrences, onRsvp })),
    );
    fireEvent.click(getByLabelText("RSVP: Tuesday relay night"));
    const card = container.querySelector('[data-series-id="series-1"]')!;
    expect(onRsvp).toHaveBeenCalledWith({
      seriesId: card.getAttribute("data-series-id"),
      occurrenceAt: card.getAttribute("data-occurrence-at"),
    });
  });
});
