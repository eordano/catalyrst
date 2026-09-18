import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../client", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../client")>();
  return { ...actual, getJSON: vi.fn(), postJSON: vi.fn(), signedGetJSON: vi.fn() };
});

import { CatalystError, getJSON, postJSON, signedGetJSON } from "../client";
import {
  commitUserAction,
  loadActiveBans,
  loadBanStatus,
  loadWarnings,
  UserActionError,
} from "./user-bans";
import type { AuthIdentity } from "../../auth/types";

const mGetJSON = vi.mocked(getJSON);
const mPostJSON = vi.mocked(postJSON);
const mSignedGetJSON = vi.mocked(signedGetJSON);

const IDENTITY: AuthIdentity = {
  signer: "0x37c7728d6f29fa22bb9e1f1aa389a61a52ffd157",
  ephemeral: { address: "0xeph", privateKey: "0xdeadbeef" },
  expiration: "2999-01-01T00:00:00.000Z",
  authChain: [],
};

const TARGET = "0x1A7c93E02b8D465F9013a6C2e74F80d5b9A3e168";
const NORMALIZED = TARGET.toLowerCase();
const BANS_URL = `/comms/users/${NORMALIZED}/bans`;
const BANS_SIGN_PATH = `/users/${NORMALIZED}/bans`;
const WARNINGS_URL = `/comms/users/${NORMALIZED}/warnings`;
const WARNINGS_SIGN_PATH = `/users/${NORMALIZED}/warnings`;

const banRow = {
  id: "142e46ea-e7f4-4d9b-bb09-694a3acca227",
  bannedAddress: NORMALIZED,
  bannedBy: IDENTITY.signer,
  reason: "harassment",
  customMessage: null,
  bannedDeviceId: null,
  bannedAt: "2026-07-26T14:29:55.392Z",
  expiresAt: "2026-07-26T15:29:55.399Z",
  liftedAt: null,
  liftedBy: null,
  createdAt: "2026-07-26T14:29:55.392Z",
};

const warningRow = {
  id: "1dda22a2-0b15-4ba3-af4c-47f90e27048c",
  warnedAddress: NORMALIZED,
  warnedBy: IDENTITY.signer,
  reason: "spam",
  warnedAt: "2026-07-26T14:30:33.923Z",
  createdAt: "2026-07-26T14:30:33.923Z",
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe("commitUserAction real catalyrst-comms writes", () => {
  it("ban POSTs the reason + duration (omitted for a permanent ban) and returns the created row", async () => {
    mPostJSON.mockResolvedValue({ data: banRow });
    const out = await commitUserAction({
      identity: IDENTITY,
      action: "ban",
      address: TARGET,
      reason: "harassment",
      durationMs: 3_600_000,
      customMessage: "  see the code of conduct  ",
    });
    expect(out.action).toBe("ban");
    expect(out.address).toBe(NORMALIZED);
    expect(out.ban?.id).toBe(banRow.id);
    expect(out.ban?.expiresAt).toBe(banRow.expiresAt);
    expect(mPostJSON).toHaveBeenCalledWith(
      BANS_URL,
      { reason: "harassment", duration: 3_600_000, customMessage: "see the code of conduct" },
      expect.objectContaining({ identity: IDENTITY, signPath: BANS_SIGN_PATH }),
    );
    await commitUserAction({
      identity: IDENTITY,
      action: "ban",
      address: TARGET,
      reason: "harassment",
      durationMs: null,
    });
    expect(mPostJSON.mock.calls[1][1]).toEqual({ reason: "harassment" });
  });

  it("warn POSTs to the warnings route and unban DELETEs the ban with no body", async () => {
    mPostJSON.mockResolvedValueOnce({ data: warningRow });
    const warned = await commitUserAction({
      identity: IDENTITY,
      action: "warn",
      address: TARGET,
      reason: "spam",
    });
    expect(warned.warning?.id).toBe(warningRow.id);
    expect(mPostJSON).toHaveBeenCalledWith(
      WARNINGS_URL,
      { reason: "spam" },
      expect.objectContaining({ identity: IDENTITY, signPath: WARNINGS_SIGN_PATH }),
    );
    mPostJSON.mockResolvedValueOnce(undefined);
    const unbanned = await commitUserAction({
      identity: IDENTITY,
      action: "unban",
      address: TARGET,
      reason: "",
    });
    expect(unbanned).toEqual({ action: "unban", address: NORMALIZED });
    expect(mPostJSON).toHaveBeenCalledWith(
      BANS_URL,
      undefined,
      expect.objectContaining({ method: "DELETE", signPath: BANS_SIGN_PATH }),
    );
  });

  it("maps 409 to already_banned and 404 to no_active_ban, and propagates a 401 gate unchanged", async () => {
    mPostJSON.mockRejectedValueOnce(
      new CatalystError("Player is already banned", BANS_URL, 409, true),
    );
    await expect(
      commitUserAction({ identity: IDENTITY, action: "ban", address: TARGET, reason: "harassment" }),
    ).rejects.toMatchObject({ status: 409, reason: "already_banned" });

    mPostJSON.mockRejectedValueOnce(new CatalystError("No active ban found", BANS_URL, 404, true));
    await expect(
      commitUserAction({ identity: IDENTITY, action: "unban", address: TARGET, reason: "" }),
    ).rejects.toBeInstanceOf(UserActionError);

    const denied = new CatalystError(
      "You are not authorized to access this resource",
      BANS_URL,
      401,
      true,
    );
    mPostJSON.mockRejectedValueOnce(denied);
    await expect(
      commitUserAction({ identity: IDENTITY, action: "ban", address: TARGET, reason: "harassment" }),
    ).rejects.toBe(denied);
  });

  it("fails closed without an identity, making no network call", async () => {
    await expect(
      commitUserAction({ identity: null, action: "ban", address: TARGET, reason: "harassment" }),
    ).rejects.toThrow(/connect your wallet/i);
    expect(mPostJSON).not.toHaveBeenCalled();
  });
});

describe("moderator reads", () => {
  it("loadActiveBans signs the read with an identity, reads unsigned when anonymous, and answers null without data", async () => {
    mSignedGetJSON.mockResolvedValueOnce({ data: [banRow] });
    const rows = await loadActiveBans({ identity: IDENTITY });
    expect(rows?.map((r) => r.id)).toEqual([banRow.id]);
    expect(mGetJSON).not.toHaveBeenCalled();
    expect(mSignedGetJSON).toHaveBeenCalledWith(
      "/comms/bans",
      expect.objectContaining({ identity: IDENTITY, signPath: "/bans" }),
    );

    mGetJSON.mockResolvedValueOnce({ data: [] });
    await loadActiveBans();
    expect(mSignedGetJSON).toHaveBeenCalledTimes(1);
    expect(mGetJSON).toHaveBeenCalledWith("/comms/bans", expect.anything());

    mSignedGetJSON.mockResolvedValueOnce({ ok: false });
    expect(await loadActiveBans({ identity: IDENTITY })).toBeNull();
  });

  it("loadBanStatus accepts the public ban shape without a device id and loadWarnings signs the per-address read", async () => {
    const { bannedDeviceId: _hidden, ...publicBan } = banRow;
    mGetJSON.mockResolvedValue({ data: { isBanned: true, ban: publicBan } });
    const status = await loadBanStatus(TARGET);
    expect(status.isBanned).toBe(true);
    expect(status.ban?.id).toBe(banRow.id);
    expect(status.ban?.name).toBeNull();
    expect(status.ban).not.toHaveProperty("bannedDeviceId");
    expect(mGetJSON).toHaveBeenCalledWith(BANS_URL, expect.anything());

    mSignedGetJSON.mockResolvedValue({ data: [warningRow] });
    const rows = await loadWarnings(TARGET, { identity: IDENTITY });
    expect(rows?.map((r) => r.id)).toEqual([warningRow.id]);
    expect(mSignedGetJSON).toHaveBeenCalledWith(
      WARNINGS_URL,
      expect.objectContaining({ signPath: WARNINGS_SIGN_PATH }),
    );
  });
});
