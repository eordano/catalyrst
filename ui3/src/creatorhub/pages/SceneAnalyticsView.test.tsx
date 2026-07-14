import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import SceneAnalyticsView from "./SceneAnalyticsView";
import {
  FIXTURE_AS_OF,
  creatorScenesStatsFixture,
  honestEmptyScene,
  zeroTrafficScene,
} from "../lib/scene-analytics.fixtures";

afterEach(() => {
  cleanup();
});

describe("SceneAnalyticsView", () => {
  describe("when signed out", () => {
    it("should show the sign-in prompt and call onConnect", () => {
      const onConnect = vi.fn();
      render(<SceneAnalyticsView phase="signed-out" onConnect={onConnect} />);
      expect(
        screen.getByText("Sign in to view your scene metrics"),
      ).toBeTruthy();
      fireEvent.click(screen.getByRole("button", { name: "Sign In" }));
      expect(onConnect).toHaveBeenCalled();
    });
  });

  describe("when loading", () => {
    it("should show a loading status", () => {
      render(<SceneAnalyticsView phase="loading" />);
      expect(screen.getByRole("status")).toBeTruthy();
    });
  });

  describe("when loading failed", () => {
    it("should show the error and retry", () => {
      const onRetry = vi.fn();
      render(
        <SceneAnalyticsView phase="error" error="boom" onRetry={onRetry} />,
      );
      expect(screen.getByRole("alert").textContent).toContain(
        "Could not load metrics",
      );
      expect(screen.getByRole("alert").textContent).toContain("boom");
      fireEvent.click(screen.getByRole("button", { name: "Retry" }));
      expect(onRetry).toHaveBeenCalled();
    });
  });

  describe("when there are no scenes", () => {
    it("should show the empty state", () => {
      render(<SceneAnalyticsView phase="ready" scenes={[]} asOf={null} />);
      expect(screen.getByText("No Places to analyse yet")).toBeTruthy();
    });
  });

  describe("when rendering the portfolio", () => {
    it("should list every scene with its display name", () => {
      render(
        <SceneAnalyticsView
          phase="ready"
          scenes={creatorScenesStatsFixture.scenes}
          asOf={FIXTURE_AS_OF}
        />,
      );
      expect(screen.getByText("Plaza Corner")).toBeTruthy();
      expect(screen.getByText("Quiet Parcel")).toBeTruthy();
      expect(screen.getByText("hidden-gem.dcl.eth")).toBeTruthy();
      expect(screen.getByText("kickoff.dcl.eth")).toBeTruthy();
      expect(screen.getByText("4 Places")).toBeTruthy();
    });

    it("should open a scene on row click", () => {
      const onOpenScene = vi.fn();
      render(
        <SceneAnalyticsView
          phase="ready"
          scenes={creatorScenesStatsFixture.scenes}
          asOf={FIXTURE_AS_OF}
          onOpenScene={onOpenScene}
        />,
      );
      fireEvent.click(screen.getByText("Plaza Corner"));
      expect(onOpenScene).toHaveBeenCalledWith(
        expect.objectContaining({ sceneType: "genesis", sceneId: "-3|-2" }),
      );
    });

    it("should filter scenes by the search query", () => {
      render(
        <SceneAnalyticsView
          phase="ready"
          scenes={creatorScenesStatsFixture.scenes}
          asOf={FIXTURE_AS_OF}
        />,
      );
      fireEvent.change(screen.getByPlaceholderText("Search"), {
        target: { value: "kickoff" },
      });
      expect(screen.getByText("kickoff.dcl.eth")).toBeTruthy();
      expect(screen.queryByText("Plaza Corner")).toBeNull();
    });
  });

  describe("when drilling into a genesis scene", () => {
    it("should render overview totals from the 30-day window", () => {
      render(
        <SceneAnalyticsView
          phase="ready"
          scenes={creatorScenesStatsFixture.scenes}
          asOf={FIXTURE_AS_OF}
          selected={{ sceneType: "genesis", sceneId: "-3|-2" }}
        />,
      );
      expect(screen.getByText("Analytics - Plaza Corner")).toBeTruthy();
      expect(screen.getByText((1500).toLocaleString())).toBeTruthy();
      expect(screen.getByText("36%")).toBeTruthy();
      expect(screen.getAllByText("Day 7 Retention").length).toBeGreaterThan(0);
      expect(screen.getByText("Social Interactions")).toBeTruthy();
    });

    it("should keep the ranking slot hidden by default", () => {
      render(
        <SceneAnalyticsView
          phase="ready"
          scenes={creatorScenesStatsFixture.scenes}
          asOf={FIXTURE_AS_OF}
          selected={{ sceneType: "genesis", sceneId: "-3|-2" }}
        />,
      );
      expect(screen.queryByText("Places Ranking")).toBeNull();
    });

    it("should go back to the list", () => {
      const onBack = vi.fn();
      render(
        <SceneAnalyticsView
          phase="ready"
          scenes={creatorScenesStatsFixture.scenes}
          asOf={FIXTURE_AS_OF}
          selected={{ sceneType: "genesis", sceneId: "-3|-2" }}
          onBack={onBack}
        />,
      );
      fireEvent.click(screen.getByRole("button", { name: "All scenes" }));
      expect(onBack).toHaveBeenCalled();
    });
  });

  describe("when drilling into a world with masked retention", () => {
    it("should show not-enough-data placeholders", () => {
      render(
        <SceneAnalyticsView
          phase="ready"
          scenes={creatorScenesStatsFixture.scenes}
          asOf={FIXTURE_AS_OF}
          selected={{ sceneType: "world", sceneId: "hidden-gem.dcl.eth" }}
        />,
      );
      expect(
        screen.getAllByText("Not enough data").length,
      ).toBeGreaterThanOrEqual(3);
    });
  });

  describe("when drilling into a zero-traffic scene", () => {
    it("should show honest zeros", () => {
      render(
        <SceneAnalyticsView
          phase="ready"
          scenes={[zeroTrafficScene]}
          asOf={FIXTURE_AS_OF}
          selected={{ sceneType: "genesis", sceneId: "10|20" }}
        />,
      );
      expect(screen.getByText("Analytics - Quiet Parcel")).toBeTruthy();
      expect(screen.getAllByText("0").length).toBeGreaterThan(0);
    });
  });

  describe("when drilling into a scene with all-null data", () => {
    it("should render em dashes without crashing", () => {
      render(
        <SceneAnalyticsView
          phase="ready"
          scenes={[honestEmptyScene]}
          asOf={FIXTURE_AS_OF}
          selected={{ sceneType: "world", sceneId: "sparse.dcl.eth" }}
        />,
      );
      expect(screen.getByText("Analytics - sparse.dcl.eth")).toBeTruthy();
      expect(screen.getAllByText("\u{2014}").length).toBeGreaterThan(0);
    });
  });

  describe("when deep linking to a scene that is not in the payload", () => {
    it("should show the no-data state instead of another scene", () => {
      render(
        <SceneAnalyticsView
          phase="ready"
          scenes={creatorScenesStatsFixture.scenes}
          asOf={FIXTURE_AS_OF}
          selected={{ sceneType: "world", sceneId: "missing.dcl.eth" }}
        />,
      );
      expect(screen.getByText("No analytics for this scene yet")).toBeTruthy();
      expect(screen.queryByText("Analytics - kickoff.dcl.eth")).toBeNull();
    });
  });

  describe("when the scene card renders a world", () => {
    it("should link Jump In to the realm URL", () => {
      render(
        <SceneAnalyticsView
          phase="ready"
          scenes={creatorScenesStatsFixture.scenes}
          asOf={FIXTURE_AS_OF}
          selected={{ sceneType: "world", sceneId: "kickoff.dcl.eth" }}
          worldAccess={{ "kickoff.dcl.eth": "private" }}
        />,
      );
      const jumpIn = screen.getByRole("link", { name: /Jump In/ });
      expect(jumpIn.getAttribute("href")).toBe(
        "https://decentraland.org/play/?realm=kickoff.dcl.eth",
      );
      expect(screen.getByText("Private")).toBeTruthy();
    });

    it("should disable Edit Scene when no edit link is available", () => {
      render(
        <SceneAnalyticsView
          phase="ready"
          scenes={creatorScenesStatsFixture.scenes}
          asOf={FIXTURE_AS_OF}
          selected={{ sceneType: "world", sceneId: "kickoff.dcl.eth" }}
        />,
      );
      const edit = screen.getByRole("button", { name: /Edit Scene/ });
      expect((edit as HTMLButtonElement).disabled).toBe(true);
    });
  });

  describe("when exporting a scene CSV", () => {
    it("should build a single 91-line blob for 90 days", async () => {
      const texts: Promise<string>[] = [];
      const originalCreate = URL.createObjectURL;
      const originalRevoke = URL.revokeObjectURL;
      URL.createObjectURL = vi.fn((blob: Blob) => {
        texts.push(blob.text());
        return "blob:mock";
      }) as typeof URL.createObjectURL;
      URL.revokeObjectURL = vi.fn() as typeof URL.revokeObjectURL;
      try {
        render(
          <SceneAnalyticsView
            phase="ready"
            scenes={creatorScenesStatsFixture.scenes}
            asOf={FIXTURE_AS_OF}
            selected={{ sceneType: "genesis", sceneId: "-3|-2" }}
          />,
        );
        fireEvent.click(
          screen.getByRole("button", { name: /Export Analytics/ }),
        );
        expect(texts).toHaveLength(1);
        const lines = (await texts[0]!).split("\n");
        expect(lines).toHaveLength(91);
        expect(lines[0]).toBe(
          "date,visits,unique_users,new_users,median_active_time_s,peak_concurrent_users,messages_sent,emotes_played",
        );
        expect(lines[lines.length - 1]).toBe("2026-07-21,50,30,5,150,27,110,55");
      } finally {
        URL.createObjectURL = originalCreate;
        URL.revokeObjectURL = originalRevoke;
      }
    });
  });
});
