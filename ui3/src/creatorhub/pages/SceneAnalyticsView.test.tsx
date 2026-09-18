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

const PORTFOLIO = {
  phase: "ready" as const,
  scenes: creatorScenesStatsFixture.scenes,
  asOf: FIXTURE_AS_OF,
};

describe("SceneAnalyticsView", () => {
  it("shows a sign-in prompt, a loading status, a retryable error, or an empty state per phase", () => {
    const onConnect = vi.fn();
    const signedOut = render(<SceneAnalyticsView phase="signed-out" onConnect={onConnect} />);
    expect(screen.getByText("Sign in to view your scene metrics")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Sign In" }));
    expect(onConnect).toHaveBeenCalled();
    signedOut.unmount();

    const loading = render(<SceneAnalyticsView phase="loading" />);
    expect(screen.getByRole("status")).toBeTruthy();
    loading.unmount();

    const onRetry = vi.fn();
    const failed = render(<SceneAnalyticsView phase="error" error="boom" onRetry={onRetry} />);
    expect(screen.getByRole("alert").textContent).toContain("Could not load metrics");
    expect(screen.getByRole("alert").textContent).toContain("boom");
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(onRetry).toHaveBeenCalled();
    failed.unmount();

    render(<SceneAnalyticsView phase="ready" scenes={[]} asOf={null} />);
    expect(screen.getByText("No Places to analyse yet")).toBeTruthy();
  });

  it("lists every scene, filters by search, and opens a scene on row click", () => {
    const onOpenScene = vi.fn();
    render(<SceneAnalyticsView {...PORTFOLIO} onOpenScene={onOpenScene} />);
    for (const name of ["Plaza Corner", "Quiet Parcel", "hidden-gem.dcl.eth", "kickoff.dcl.eth", "4 Places"]) {
      expect(screen.getByText(name)).toBeTruthy();
    }
    fireEvent.click(screen.getByText("Plaza Corner"));
    expect(onOpenScene).toHaveBeenCalledWith(
      expect.objectContaining({ sceneType: "genesis", sceneId: "-3|-2" }),
    );
    fireEvent.change(screen.getByPlaceholderText("Search"), {
      target: { value: "kickoff" },
    });
    expect(screen.getByText("kickoff.dcl.eth")).toBeTruthy();
    expect(screen.queryByText("Plaza Corner")).toBeNull();
  });

  it("drills into a genesis scene with 30-day totals, no ranking slot, and a way back", () => {
    const onBack = vi.fn();
    render(
      <SceneAnalyticsView
        {...PORTFOLIO}
        selected={{ sceneType: "genesis", sceneId: "-3|-2" }}
        onBack={onBack}
      />,
    );
    expect(screen.getByText("Analytics - Plaza Corner")).toBeTruthy();
    expect(screen.getByText((1500).toLocaleString())).toBeTruthy();
    expect(screen.getByText("36%")).toBeTruthy();
    expect(screen.getAllByText("Day 7 Retention").length).toBeGreaterThan(0);
    expect(screen.getByText("Social Interactions")).toBeTruthy();
    expect(screen.queryByText("Places Ranking")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "All scenes" }));
    expect(onBack).toHaveBeenCalled();
  });

  it("stays honest on masked retention, zero traffic, all-null data, and a scene missing from the payload", () => {
    const masked = render(
      <SceneAnalyticsView {...PORTFOLIO} selected={{ sceneType: "world", sceneId: "hidden-gem.dcl.eth" }} />,
    );
    expect(screen.getAllByText("Not enough data").length).toBeGreaterThanOrEqual(3);
    masked.unmount();

    const zero = render(
      <SceneAnalyticsView
        phase="ready"
        scenes={[zeroTrafficScene]}
        asOf={FIXTURE_AS_OF}
        selected={{ sceneType: "genesis", sceneId: "10|20" }}
      />,
    );
    expect(screen.getByText("Analytics - Quiet Parcel")).toBeTruthy();
    expect(screen.getAllByText("0").length).toBeGreaterThan(0);
    zero.unmount();

    const sparse = render(
      <SceneAnalyticsView
        phase="ready"
        scenes={[honestEmptyScene]}
        asOf={FIXTURE_AS_OF}
        selected={{ sceneType: "world", sceneId: "sparse.dcl.eth" }}
      />,
    );
    expect(screen.getByText("Analytics - sparse.dcl.eth")).toBeTruthy();
    expect(screen.getAllByText("\u{2014}").length).toBeGreaterThan(0);
    sparse.unmount();

    render(
      <SceneAnalyticsView {...PORTFOLIO} selected={{ sceneType: "world", sceneId: "missing.dcl.eth" }} />,
    );
    expect(screen.getByText("No analytics for this scene yet")).toBeTruthy();
    expect(screen.queryByText("Analytics - kickoff.dcl.eth")).toBeNull();
  });

  it("links Jump In to the realm URL for a world and disables Edit Scene without an edit link", () => {
    render(
      <SceneAnalyticsView
        {...PORTFOLIO}
        selected={{ sceneType: "world", sceneId: "kickoff.dcl.eth" }}
        worldAccess={{ "kickoff.dcl.eth": "private" }}
      />,
    );
    const jumpIn = screen.getByRole("link", { name: /Jump In/ });
    expect(jumpIn.getAttribute("href")).toBe(
      "https://decentraland.org/play/?realm=kickoff.dcl.eth",
    );
    expect(screen.getByText("Private")).toBeTruthy();
    const edit = screen.getByRole("button", { name: /Edit Scene/ });
    expect((edit as HTMLButtonElement).disabled).toBe(true);
  });

  it("exports a single 91-line CSV blob for 90 days", async () => {
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
        <SceneAnalyticsView {...PORTFOLIO} selected={{ sceneType: "genesis", sceneId: "-3|-2" }} />,
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
