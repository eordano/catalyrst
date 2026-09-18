import { describe, test, expect } from "vitest";
import { FakeBridge, makeFriend } from "./fakeBridge";

describe("FakeBridge contract", () => {
  test("onState delivers pushes to live subscribers only, never replays, and wrapDispatch wraps every delivery", () => {
    const bridge = new FakeBridge();
    bridge.pushIdentity();
    const seen: unknown[] = [];
    const unsub = bridge.onState((p) => seen.push(p));
    expect(seen).toHaveLength(0);

    bridge.pushScene({ title: "A" });
    expect(seen).toHaveLength(1);
    expect(bridge.subscriberCount).toBe(1);

    unsub();
    bridge.pushScene({ title: "B" });
    expect(seen).toHaveLength(1);
    expect(bridge.subscriberCount).toBe(0);

    let wrapped = 0;
    bridge.wrapDispatch = (fn) => {
      wrapped += 1;
      fn();
    };
    bridge.onState(() => {});
    bridge.pushMic({});
    bridge.pushLoginCode({});
    expect(wrapped).toBe(2);
  });

  test("send records typed entries in order; expectSent/expectNotSent match partial payloads and predicates; clearSent forgets; push helpers build shaped variants", () => {
    const bridge = new FakeBridge();
    bridge.send("SendChat", { channel: "Nearby", message: "hi" });
    bridge.send("Teleport", { x: 8, z: 8 });
    bridge.send("SetSetting", { name: "Bloom", value: 2 });
    expect(bridge.sent.map((s) => s.action)).toEqual(["SendChat", "Teleport", "SetSetting"]);
    expect(bridge.sentOf("Teleport")).toEqual([{ x: 8, z: 8 }]);
    expect(bridge.lastSent("SendChat")).toEqual({ channel: "Nearby", message: "hi" });
    expect(bridge.lastSent("PlayEmote")).toBeUndefined();

    expect(bridge.expectSent("SetSetting", { name: "Bloom" })).toEqual({ name: "Bloom", value: 2 });
    expect(bridge.expectSent("SetSetting", (p) => p.value === 2)).toBeTruthy();
    expect(() => bridge.expectSent("SetSetting", { name: "Fog" })).toThrow(/expected bridge send/);
    expect(() => bridge.expectSent("StopEmote")).toThrow(/expected bridge send/);
    expect(() => bridge.expectNotSent("SetSetting")).toThrow(/expected NO/);
    bridge.expectNotSent("StopEmote");

    bridge.clearSent();
    expect(bridge.sent).toHaveLength(0);

    expect(bridge.pushIdentity({ name: "Ada" })).toMatchObject({ kind: "identity", name: "Ada", isGuest: false });
    expect(
      bridge.pushFriends({ friends: [makeFriend({ status: "online" }), makeFriend({ status: "offline" })] }),
    ).toMatchObject({ kind: "friends", onlineCount: 1 });
    expect(bridge.pushFriends({ friends: [], onlineCount: 9 }).onlineCount).toBe(9);
    expect(bridge.pushLoading({})).toMatchObject({ kind: "loading", ready: true, avatarLoaded: true });
    expect(bridge.pushConnection({ sceneHealth: "error" })).toMatchObject({
      kind: "connection",
      sceneHealth: "error",
      globalRoom: true,
    });
    expect(bridge.pushChat({ message: "yo" })).toMatchObject({ kind: "chat", message: "yo", channel: "Nearby" });
    expect(bridge.pushScene({})).toMatchObject({ kind: "scene", realm: "main" });
  });
});
