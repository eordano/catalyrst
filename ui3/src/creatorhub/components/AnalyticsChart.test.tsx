import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import AnalyticsChart from "./AnalyticsChart";

afterEach(() => {
  cleanup();
});

const RUBY = "var(--brand)";

function makePoints(values: Array<number | null>, endDate = "2026-07-21") {
  const base = new Date(`${endDate}T00:00:00Z`);
  return values.map((value, index) => {
    const d = new Date(base);
    d.setUTCDate(d.getUTCDate() - (values.length - 1 - index));
    return { date: d.toISOString().slice(0, 10), value };
  });
}

describe("AnalyticsChart", () => {
  it("draws a line and area with the fixed percent axis, legend chips for two series, and nothing for masked or empty series", () => {
    const single = render(
      <AnalyticsChart
        series={[{ key: "d7", label: "Day 7 Retention", color: RUBY, points: makePoints([30, 35, 33, 38, 41]) }]}
        ariaLabel="Day 7 Retention"
        unit="%"
        yMax={80}
        yStep={20}
        area
      />,
    );
    expect(single.container.querySelectorAll(".series-line")).toHaveLength(1);
    expect(single.container.querySelectorAll(".series-area")).toHaveLength(1);
    expect(single.container.querySelectorAll(".grid-line")).toHaveLength(5);
    expect(single.container.textContent).toContain("80%");

    const two = render(
      <AnalyticsChart
        series={[
          { key: "messages", label: "Messages Sent", color: "#2196F3", points: makePoints([10, 12, 14]) },
          { key: "emotes", label: "Emotes Played", color: "#34CE77", points: makePoints([5, 6, 7]) },
        ]}
        ariaLabel="Social Interactions"
        legend
      />,
    );
    expect(two.container.querySelectorAll(".series-line")).toHaveLength(2);
    expect(two.container.querySelectorAll(".legend-chip")).toHaveLength(2);
    expect(two.container.textContent).toContain("Messages Sent");
    expect(two.container.textContent).toContain("Emotes Played");

    const masked = render(
      <AnalyticsChart
        series={[{ key: "d1", label: "Day 1 Retention", color: RUBY, points: makePoints([null, null, null]) }]}
        ariaLabel="Day 1 Retention"
        unit="%"
      />,
    );
    expect(masked.container.querySelectorAll(".series-line")).toHaveLength(0);
    const empty = render(
      <AnalyticsChart series={[{ key: "x", label: "X", color: RUBY, points: [] }]} ariaLabel="X" />,
    );
    expect(empty.container.querySelectorAll(".series-line")).toHaveLength(0);
  });

  it("shows a tooltip with value and delta on keyboard navigation and clears it on Escape", () => {
    const { container } = render(
      <AnalyticsChart
        series={[{ key: "d7", label: "Day 7 Retention", color: RUBY, points: makePoints([30, 35, 41]) }]}
        ariaLabel="Day 7 Retention"
        unit="%"
        yMax={80}
        yStep={20}
        showDelta
      />,
    );
    const svg = container.querySelector("svg")!;
    fireEvent.keyDown(svg, { key: "ArrowRight" });
    expect(container.querySelector(".tooltip")).not.toBeNull();
    expect(container.textContent).toContain("Jul 21");
    expect(container.textContent).toContain("Day 7 Retention 41%");
    expect(container.textContent).toContain("+6%");
    fireEvent.keyDown(svg, { key: "Escape" });
    expect(container.querySelector(".tooltip")).toBeNull();
  });
});
