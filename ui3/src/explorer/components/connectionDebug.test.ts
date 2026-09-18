import { describe, expect, test } from "vitest";

import {
  aboutMatchesRealm,
  currentSceneId,
  debugLines,
  debugText,
  EMPTY_ABOUT,
  formatPosition,
  overlayBuildId,
  parseLiveScenes,
  parseRealmAbout,
  realmAboutBase,
  type DebugInfo,
} from "./connectionDebug";

const ABOUT_WIRE = {
  healthy: true,
  content: { healthy: true, version: "8.0.3+rust", commitHash: "unknown" },
  lambdas: { healthy: true, version: "4.12.0+rust" },
  configurations: { networkId: 1, realmName: "dcl-one" },
  comms: {
    healthy: true,
    protocol: "v3",
    version: "24.18.0+pulse-dev",
    usersCount: 7,
    adapter: "archipelago:archipelago:wss://catalyst.example.com/ws",
    fixedAdapter: "archipelago:archipelago:wss://catalyst.example.com/ws",
  },
};

function info(over: Partial<DebugInfo> = {}): DebugInfo {
  return {
    realm: "dcl-one",
    parcel: "-141,98",
    position: [-2249.4, 1.2, 1571.9],
    heading: 271.6,
    sceneTitle: "CBD Plaza",
    sceneCoords: "-147,91",
    sceneId: "bafkreibg66k5ipkfaego4qccbesmtfcpa6xmpzkmghm27qbl7pxbq7g5wm",
    connection: { sceneHealth: "ok", sceneRoom: true, globalRoom: false },
    about: parseRealmAbout(ABOUT_WIRE),
    address: "0x92de52247aeae00fcfb18072c8564f3549b64f9c",
    overlayBuild: "AppShell-CSUtUagR",
    userAgent: "Mozilla/5.0 test",
    fps: { page: 58, engine: 61, ms: 17.2 },
    at: new Date("2026-09-17T17:01:29.000Z"),
    ...over,
  };
}

describe("parseRealmAbout", () => {
  test("projects the /about fields the debug block shows", () => {
    expect(parseRealmAbout(ABOUT_WIRE)).toEqual({
      realmName: "dcl-one",
      commsProtocol: "v3",
      commsAdapter: "archipelago:archipelago:wss://catalyst.example.com/ws",
      commsVersion: "24.18.0+pulse-dev",
      contentVersion: "8.0.3+rust",
      lambdasVersion: "4.12.0+rust",
      usersCount: 7,
    });
  });

  test("falls back to fixedAdapter and tolerates garbage", () => {
    const only = parseRealmAbout({ comms: { fixedAdapter: "ws-room:wss://x/rooms/1" } });
    expect(only.commsAdapter).toBe("ws-room:wss://x/rooms/1");
    expect(parseRealmAbout(null)).toEqual(EMPTY_ABOUT);
    expect(parseRealmAbout("nope")).toEqual(EMPTY_ABOUT);
    expect(parseRealmAbout({ comms: { usersCount: "7" } }).usersCount).toBeNull();
  });
});

describe("overlayBuildId", () => {
  test("is the hashed chunk name of the module that carries the component", () => {
    expect(overlayBuildId("https://catalyst.example.com/play/ui3-overlay/chunks/AppShell-CSUtUagR.js")).toBe(
      "AppShell-CSUtUagR",
    );
    expect(overlayBuildId("https://catalyst.example.com/play/ui3-overlay/overlay.js?v=3")).toBe("overlay");
    expect(overlayBuildId("file:///src/explorer/components/ConnectionStatus.tsx")).toBe(
      "ConnectionStatus",
    );
    expect(overlayBuildId("")).toBe("dev");
  });
});

describe("live scenes", () => {
  const LISTING = [
    "CBD Plaza [bafkreicbd]",
    "Quest HUD [bafkreiquest, portable]",
    "Broken Corner [bafkreibroken, broken]",
  ].join("\n");

  test("parses the /live_scenes console listing", () => {
    expect(parseLiveScenes(LISTING)).toEqual([
      { title: "CBD Plaza", hash: "bafkreicbd", portable: false, broken: false },
      { title: "Quest HUD", hash: "bafkreiquest", portable: true, broken: false },
      { title: "Broken Corner", hash: "bafkreibroken", portable: false, broken: true },
    ]);
    expect(parseLiveScenes("no scenes loaded")).toEqual([]);
  });

  test("picks the scene whose title the bridge reports, never a portable", () => {
    const scenes = parseLiveScenes(LISTING);
    expect(currentSceneId(scenes, "CBD Plaza")).toBe("bafkreicbd");
    expect(currentSceneId(scenes, "Quest HUD")).toBeNull();
    expect(currentSceneId(scenes, "elsewhere")).toBeNull();
    expect(currentSceneId(parseLiveScenes("Solo [bafkreisolo]"), null)).toBe("bafkreisolo");
  });
});

describe("debug block", () => {
  test("formats the position to one decimal", () => {
    expect(formatPosition([-2249.44, 1.25, 1571.96])).toBe("-2249.4, 1.3, 1572.0");
    expect(formatPosition(null)).toBeNull();
  });

  test("lists realm, parcel, position, scene, comms, versions, build and fps", () => {
    expect(Object.fromEntries(debugLines(info()))).toEqual({
      Realm: "dcl-one",
      Parcel: "-141,98",
      Position: "-2249.4, 1.2, 1571.9 \u00b7 heading 272\u00b0",
      Scene: "CBD Plaza (-147,91)",
      "Scene id": "bafkreibg66k5ipkfaego4qccbesmtfcpa6xmpzkmghm27qbl7pxbq7g5wm",
      "Scene health": "ok",
      "Scene room": "connected",
      "Global room": "none",
      Comms: "v3 \u00b7 archipelago:archipelago:wss://catalyst.example.com/ws",
      "Users online": "7",
      Server: "content 8.0.3+rust \u00b7 lambdas 4.12.0+rust \u00b7 comms 24.18.0+pulse-dev",
      "Overlay build": "AppShell-CSUtUagR",
      Address: "0x92de52247aeae00fcfb18072c8564f3549b64f9c",
      FPS: "page 58 \u00b7 engine 61 \u00b7 17.2 ms/frame",
      Browser: "Mozilla/5.0 test",
      Captured: "2026-09-17T17:01:29.000Z",
    });
  });

  test("omits what is unknown instead of printing placeholders", () => {
    const keys = debugLines(
      info({
        realm: null,
        parcel: null,
        position: null,
        sceneTitle: null,
        sceneCoords: null,
        sceneId: null,
        connection: null,
        about: null,
        address: null,
        fps: null,
        userAgent: null,
      }),
    ).map(([k]) => k);
    expect(keys).toEqual(["Overlay build", "Captured"]);
    expect(debugLines(info({ overlayBuild: null, at: null, fps: null, about: null, connection: null, realm: null, parcel: null, position: null, sceneTitle: null, sceneCoords: null, sceneId: null, address: null, userAgent: null }))).toEqual([]);
  });

  test("keeps the build and capture time out of a render that has not mounted", () => {
    const keys = debugLines(info({ overlayBuild: null, at: null, userAgent: null })).map(([k]) => k);
    expect(keys).not.toContain("Overlay build");
    expect(keys).not.toContain("Captured");
    expect(keys).not.toContain("Browser");
    expect(keys).toContain("FPS");
  });

  test("falls back to the realm name from /about", () => {
    expect(Object.fromEntries(debugLines(info({ realm: null }))).Realm).toBe("dcl-one");
  });

  test("copies as key: value lines", () => {
    const text = debugText(info());
    expect(text.split("\n")[0]).toBe("Realm: dcl-one");
    expect(text).toContain("\nFPS: page 58 \u00b7 engine 61 \u00b7 17.2 ms/frame\n");
  });
});

describe("realmAboutBase", () => {
  test("turns a realm url from the engine into the base its /about lives under", () => {
    expect(realmAboutBase("https://peer.example.org")).toBe("https://peer.example.org");
    expect(realmAboutBase("https://peer.example.org/")).toBe("https://peer.example.org");
    expect(realmAboutBase("https://peer.example.org/about")).toBe("https://peer.example.org");
    expect(realmAboutBase("http://localhost:5100/world/foo/about/")).toBe("http://localhost:5100/world/foo");
  });

  test("is null for a realm name, an empty value or a non-http scheme", () => {
    expect(realmAboutBase("dcl-one")).toBeNull();
    expect(realmAboutBase("")).toBeNull();
    expect(realmAboutBase(null)).toBeNull();
    expect(realmAboutBase("ws://peer.example.org")).toBeNull();
  });
});

describe("aboutMatchesRealm", () => {
  const about = parseRealmAbout(ABOUT_WIRE);

  test("a named realm only accepts an /about that names the same realm", () => {
    expect(aboutMatchesRealm(about, "dcl-one")).toBe(true);
    expect(aboutMatchesRealm(about, "somewhere-else")).toBe(false);
    expect(aboutMatchesRealm({ ...about, realmName: null }, "somewhere-else")).toBe(true);
  });

  test("a realm url was fetched from that url, so it always matches", () => {
    expect(aboutMatchesRealm(about, "https://peer.example.org")).toBe(true);
    expect(aboutMatchesRealm(about, null)).toBe(true);
  });
});
