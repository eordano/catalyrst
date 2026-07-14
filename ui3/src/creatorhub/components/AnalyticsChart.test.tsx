import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import AnalyticsChart from "./AnalyticsChart";
import type { ChartSeries } from "../lib/scene-analytics";

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
  describe("when rendering a single series with an area", () => {
    it("should draw one line, an area, and the fixed percent axis", () => {
      const series: ChartSeries[] = [
        {
          key: "d7",
          label: "Day 7 Retention",
          color: RUBY,
          points: makePoints([30, 35, 33, 38, 41]),
        },
      ];
      const { container } = render(
        <AnalyticsChart
          series={series}
          ariaLabel="Day 7 Retention"
          unit="%"
          yMax={80}
          yStep={20}
          area
        />,
      );
      expect(container.querySelectorAll(".series-line")).toHaveLength(1);
      expect(container.querySelectorAll(".series-area")).toHaveLength(1);
      expect(container.querySelectorAll(".grid-line")).toHaveLength(5);
      expect(container.textContent).toContain("80%");
    });
  });

  describe("when every point is masked", () => {
    it("should draw no lines", () => {
      const series: ChartSeries[] = [
        {
          key: "d1",
          label: "Day 1 Retention",
          color: RUBY,
          points: makePoints([null, null, null]),
        },
      ];
      const { container } = render(
        <AnalyticsChart series={series} ariaLabel="Day 1 Retention" unit="%" />,
      );
      expect(container.querySelectorAll(".series-line")).toHaveLength(0);
    });
  });

  describe("when rendering two series with a legend", () => {
    it("should draw both lines and legend chips", () => {
      const series: ChartSeries[] = [
        {
          key: "messages",
          label: "Messages Sent",
          color: "#2196F3",
          points: makePoints([10, 12, 14]),
        },
        {
          key: "emotes",
          label: "Emotes Played",
          color: "#34CE77",
          points: makePoints([5, 6, 7]),
        },
      ];
      const { container } = render(
        <AnalyticsChart series={series} ariaLabel="Social Interactions" legend />,
      );
      expect(container.querySelectorAll(".series-line")).toHaveLength(2);
      expect(container.querySelectorAll(".legend-chip")).toHaveLength(2);
      expect(container.textContent).toContain("Messages Sent");
      expect(container.textContent).toContain("Emotes Played");
    });
  });

  describe("when navigating with the keyboard", () => {
    it("should show a tooltip with value and delta, and clear on Escape", () => {
      const series: ChartSeries[] = [
        {
          key: "d7",
          label: "Day 7 Retention",
          color: RUBY,
          points: makePoints([30, 35, 41]),
        },
      ];
      const { container } = render(
        <AnalyticsChart
          series={series}
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

  describe("when the series has no points", () => {
    it("should render no lines", () => {
      const { container } = render(
        <AnalyticsChart
          series={[{ key: "x", label: "X", color: RUBY, points: [] }]}
          ariaLabel="X"
        />,
      );
      expect(container.querySelectorAll(".series-line")).toHaveLength(0);
    });
  });
});
