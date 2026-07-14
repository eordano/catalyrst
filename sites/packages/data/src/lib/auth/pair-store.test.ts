import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";
import { describe, expect, it } from "vitest";

import { action } from "@routes/routes/internal.pair";
import { buildEphemeralMessage, generateEphemeralKey } from "./identity";
import { allowPairCreate, createMemoryPairStore } from "./pair-store.server";
import type { PairStore } from "./pair-store.server";

function mustCreate(store: PairStore) {
  const session = store.create({
    ephemeral: "0x" + "1".repeat(40),
    expiration: futureIso(),
    message: "m",
  });
  if (!session) throw new Error("expected create() to return a session");
  return session;
}

function post(body: unknown, ip = "test-suite"): Promise<Response> {
  return action({
    request: new Request("http://localhost/internal/pair", {
      method: "POST",
      headers: { "content-type": "application/json", "x-forwarded-for": ip },
      body: JSON.stringify(body),
    }),
  }) as Promise<Response>;
}

function futureIso(ms = 60 * 60 * 1000): string {
  return new Date(Date.now() + ms).toISOString();
}

async function createSession(): Promise<{
  id: string;
  pollToken: string;
  ephemeral: string;
  expiration: string;
}> {
  const ephemeral = generateEphemeralKey().address;
  const expiration = futureIso();
  const res = await post({ kind: "create", ephemeral, expiration });
  expect(res.status).toBe(200);
  const body = (await res.json()) as { id: string; pollToken: string };
  return { ...body, ephemeral, expiration };
}

describe("phone pairing API", () => {
  it("runs the full rail: create, phone signs, poll releases once", async () => {
    const { id, pollToken, ephemeral, expiration } = await createSession();
    expect(id).toMatch(/^[A-Za-z0-9_-]{20,}$/);
    expect(pollToken).toMatch(/^[A-Za-z0-9_-]{40,}$/);

    const pending = await post({ kind: "poll", id, pollToken });
    expect(pending.status).toBe(200);
    expect(await pending.json()).toEqual({ state: "pending" });

    const phone = privateKeyToAccount(generatePrivateKey());
    const message = buildEphemeralMessage(ephemeral, new Date(expiration));
    const signature = await phone.signMessage({ message });
    const completed = await post({
      kind: "complete",
      id,
      signer: phone.address,
      signature,
    });
    expect(completed.status).toBe(200);

    const released = await post({ kind: "poll", id, pollToken });
    expect(released.status).toBe(200);
    const payload = (await released.json()) as {
      state: string;
      signer: string;
      signature: string;
    };
    expect(payload.state).toBe("completed");
    expect(payload.signer).toBe(phone.address.toLowerCase());
    expect(payload.signature).toBe(signature);

    const again = await post({ kind: "poll", id, pollToken });
    expect(again.status).toBe(404);
  });

  it("rejects a signature that does not recover to the claimed signer", async () => {
    const { id, ephemeral, expiration } = await createSession();
    const phone = privateKeyToAccount(generatePrivateKey());
    const impostor = privateKeyToAccount(generatePrivateKey());
    const message = buildEphemeralMessage(ephemeral, new Date(expiration));
    const signature = await phone.signMessage({ message });
    const res = await post({
      kind: "complete",
      id,
      signer: impostor.address,
      signature,
    });
    expect(res.status).toBe(400);
  });

  it("rejects a signature over a different message", async () => {
    const { id } = await createSession();
    const phone = privateKeyToAccount(generatePrivateKey());
    const signature = await phone.signMessage({ message: "something else" });
    const res = await post({
      kind: "complete",
      id,
      signer: phone.address,
      signature,
    });
    expect(res.status).toBe(400);
  });

  it("is one-shot on completion: a second complete gets 409", async () => {
    const { id, ephemeral, expiration } = await createSession();
    const phone = privateKeyToAccount(generatePrivateKey());
    const message = buildEphemeralMessage(ephemeral, new Date(expiration));
    const signature = await phone.signMessage({ message });
    expect(
      (await post({ kind: "complete", id, signer: phone.address, signature }))
        .status,
    ).toBe(200);
    expect(
      (await post({ kind: "complete", id, signer: phone.address, signature }))
        .status,
    ).toBe(409);
  });

  it("refuses polls with the wrong token and never leaks the result", async () => {
    const { id } = await createSession();
    const res = await post({
      kind: "poll",
      id,
      pollToken: "A".repeat(43),
    });
    expect(res.status).toBe(403);
  });

  it("validates create inputs", async () => {
    expect(
      (await post({ kind: "create", ephemeral: "nope", expiration: futureIso() }))
        .status,
    ).toBe(400);
    expect(
      (
        await post({
          kind: "create",
          ephemeral: generateEphemeralKey().address,
          expiration: new Date(Date.now() - 1000).toISOString(),
        })
      ).status,
    ).toBe(400);
    expect((await post({ kind: "nonsense" })).status).toBe(400);
  });

  it("cancel drops the session", async () => {
    const { id, pollToken } = await createSession();
    expect((await post({ kind: "cancel", id, pollToken })).status).toBe(204);
    expect((await post({ kind: "poll", id, pollToken })).status).toBe(404);
  });
});

describe("pair store", () => {
  it("expires sessions after the TTL", () => {
    const store = createMemoryPairStore(-1);
    const session = mustCreate(store);
    expect(store.poll(session.id, session.pollToken)).toEqual({
      state: "missing",
    });
    expect(store.complete(session.id, "0x" + "2".repeat(40), "0xsig")).toBe(
      "missing",
    );
  });

  it("keeps completed sessions until the desktop consumes them", () => {
    const store = createMemoryPairStore();
    const s = mustCreate(store);
    expect(store.complete(s.id, "0xabc", "0xsig")).toBe("ok");
    expect(store.get(s.id)?.completed).toEqual({
      signer: "0xabc",
      signature: "0xsig",
    });
    expect(store.poll(s.id, s.pollToken)).toEqual({
      state: "completed",
      signer: "0xabc",
      signature: "0xsig",
    });
    expect(store.get(s.id)).toBeNull();
  });

  it("refuses new sessions when full instead of evicting live ones", () => {
    const store = createMemoryPairStore();
    const first = mustCreate(store);
    for (let i = 0; i < 499; i++) mustCreate(store);
    expect(store.create({
      ephemeral: "0x" + "1".repeat(40),
      expiration: futureIso(),
      message: "m",
    })).toBeNull();
    expect(store.get(first.id)).not.toBeNull();
  });

  it("rate-limits session creation per IP", () => {
    const ip = `rl-${Math.random()}`;
    for (let i = 0; i < 30; i++) expect(allowPairCreate(ip)).toBe(true);
    expect(allowPairCreate(ip)).toBe(false);
    expect(allowPairCreate(`${ip}-other`)).toBe(true);
  });
});
