import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { fireEvent, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import ChromeShell from "../components/ChromeShell";
import FdContinuityPage, {
  type FdContinuitySceneVM,
} from "./pages/FdContinuityPage";
import FdExchangePage, {
  FD_BODY_MAX,
  type FdExchangePageProps,
} from "./pages/FdExchangePage";
import FdResponse, { type FdResponseProps } from "./pages/FdResponse";

// The 2026-08-20 persona journeys' UX traps, kept fixed: the ask form's limit
// is visible while typing and unmissable on failure, the write cap names its
// wait, the readings page says what it is and where a player responds, the
// continuity bundle is readable without a forced download, and the shell nav
// collapses instead of clipping at phone widths.

const noop = () => undefined;

function exchangeProps(
  overrides: Partial<FdExchangePageProps>,
): FdExchangePageProps {
  return {
    requests: [],
    postOpen: true,
    onTogglePost: noop,
    onPledge: noop,
    onWithdraw: noop,
    onPost: noop,
    ...overrides,
  };
}

describe("FdExchangePage ask form limit", () => {
  it("counts characters live and flags the over-limit state", () => {
    const { container } = render(
      createElement(FdExchangePage, exchangeProps({})),
    );
    const count = container.querySelector("#fd-req-body-count")!;
    expect(count.textContent).toBe(`0 / ${FD_BODY_MAX}`);
    expect(count.className).not.toContain("is-over");

    const textarea = container.querySelector<HTMLTextAreaElement>("#fd-req-body")!;
    fireEvent.change(textarea, { target: { value: "x".repeat(297) } });
    expect(count.textContent).toBe(`297 / ${FD_BODY_MAX}`);
    expect(count.className).toContain("is-over");
  });

  it("an over-limit submit fails loudly at the field and the button, keeping the text", () => {
    const onPost = vi.fn();
    const { container, getAllByRole } = render(
      createElement(FdExchangePage, exchangeProps({ onPost })),
    );
    const textarea = container.querySelector<HTMLTextAreaElement>("#fd-req-body")!;
    fireEvent.change(textarea, { target: { value: "y".repeat(297) } });
    fireEvent.submit(container.querySelector("form")!);

    expect(onPost).not.toHaveBeenCalled();
    expect(textarea.value).toBe("y".repeat(297));
    const alerts = getAllByRole("alert").map((el) => el.textContent ?? "");
    const overLimit = alerts.filter((t) =>
      t.includes(`Keep the description under ${FD_BODY_MAX} characters`),
    );
    // Once under the field, once beside the submit button.
    expect(overLimit.length).toBe(2);
    expect(overLimit[0]).toContain("it is 297 now");
    expect(overLimit[0]).toContain("your text is kept");
  });

  it("an under-limit submit posts the typed values", () => {
    const onPost = vi.fn();
    const { container } = render(
      createElement(FdExchangePage, exchangeProps({ onPost })),
    );
    fireEvent.change(container.querySelector("#fd-req-title")!, {
      target: { value: "A title" },
    });
    fireEvent.change(container.querySelector("#fd-req-body")!, {
      target: { value: "Short enough." },
    });
    fireEvent.submit(container.querySelector("form")!);
    expect(onPost).toHaveBeenCalledWith({
      title: "A title",
      body: "Short enough.",
      source: "",
    });
  });

  it("the write-cap message renders beside the submit button too", () => {
    const message =
      "Too many writes from this session — wait a minute, then post again. Your draft is still in the form.";
    const { container } = render(
      createElement(FdExchangePage, exchangeProps({ error: message })),
    );
    const inline = container.querySelector(".fd-form__alert");
    expect(inline?.textContent).toBe(message);
  });
});

describe("FdContinuityPage bundle view", () => {
  const scene: FdContinuitySceneVM = {
    id: "bastion-row",
    title: "bastion-row",
    worldName: null,
    deployedAt: null,
    importedAt: null,
    sizeBytes: null,
    parcels: null,
    source: "repo",
    sourceNote: "",
    counts: {
      changelog: 0,
      reports: 0,
      reportsAll: 0,
      episodes: 0,
      docs: 0,
      stewards: 0,
    },
    exportHref: "/foundry/continuity/bastion-row/export",
  };

  it("offers an in-browser view beside every bundle download", () => {
    const html = renderToStaticMarkup(
      createElement(FdContinuityPage, {
        scenes: [scene],
        selected: "bastion-row",
        selectedMissing: false,
        detail: {
          scene,
          memory: [],
          stewards: { active: [], past: [] },
          transfers: [],
        },
      }),
    );
    const views = html.match(/View it in the browser/g) ?? [];
    expect(views.length).toBe(2);
    expect(html).toContain('href="/foundry/continuity/bastion-row/export?view"');
    expect(html).toContain("Download the bundle");
  });
});

describe("FdResponse framing", () => {
  const props: FdResponseProps = {
    title: "Bastion Row",
    slug: "bastion-row",
    gameHref: "/foundry/play/bastion-row",
    measuredSince: "15 Aug 2026",
    signals: null,
    gatherings: [],
    runs: [],
    marketCell: null,
    emotionalJobs: null,
    cellGaps: null,
    jobGaps: null,
    askAnswers: [],
    gddHref: null,
    memory: [],
    hasVisitorNote: false,
    revision: { kind: "none" },
  };

  it("presents itself as readings and points players at the exchange", () => {
    const html = renderToStaticMarkup(createElement(FdResponse, props));
    expect(html).toContain("Readings");
    expect(html).toContain("To respond as a player");
    expect(html).toContain(
      '<a href="/foundry/exchange">post an ask or pledge on the Exchange</a>',
    );
  });
});

describe("ChromeShell narrow-width menu", () => {
  const tabs = [
    { id: "overview", label: "Overview", href: "/foundry" },
    { id: "play", label: "Play", href: "/foundry/play" },
  ] as const;

  it("renders a disclosure named after the active section, opening the full list", () => {
    const onNavigate = vi.fn();
    const { container } = render(
      createElement(ChromeShell, {
        tabs,
        active: "overview",
        onNavigate,
        tabsLabel: "Sections",
        footer: false,
      }),
    );
    const button = container.querySelector<HTMLButtonElement>(".cs__menubtn")!;
    expect(button.textContent).toContain("Overview");
    expect(button.getAttribute("aria-expanded")).toBe("false");
    expect(container.querySelector(".cs__menu")).toBeNull();

    fireEvent.click(button);
    expect(button.getAttribute("aria-expanded")).toBe("true");
    const menu = container.querySelector(".cs__menu")!;
    expect(menu.querySelectorAll(".cs__menuitem").length).toBe(2);

    fireEvent.click(menu.querySelectorAll(".cs__menuitem")[1]!);
    expect(onNavigate).toHaveBeenCalledWith("/foundry/play");
    expect(container.querySelector(".cs__menu")).toBeNull();
  });
});
