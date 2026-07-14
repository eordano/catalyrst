import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";
import { createActor } from "xstate";

import {
  checkoutMachine,
  FIRST_STEP_SLUG,
  type FulfillFn,
} from "@features/stories/marketplace/checkout/machine";

describe("GUARD A \u{2014} checkout is one-step (single CONFIRM: review \u{2192} fulfilling)", () => {
  const neverSettles: FulfillFn = () => new Promise<never>(() => {});

  it("reaches `fulfilling` from `review` on a SINGLE CONFIRM (behavioral)", () => {
    const actor = createActor(checkoutMachine, {
      input: {
        totalCredits: "10",
        idempotencyKey: "k",
        trackCtx: { sid: "sid-test", story: "buy-min-clicks" },
        run: neverSettles,
        track: () => {},
      },
    });
    actor.start();

    expect(actor.getSnapshot().value).toBe("review");

    actor.send({ type: "CONFIRM" });

    expect(actor.getSnapshot().value).toBe("fulfilling");

    actor.stop();
  });

  it("has NO intermediate confirm state and review\u{2192}CONFIRM targets `fulfilling` (structural)", () => {
    type ConfigView = {
      initial: string;
      states: Record<
        string,
        { on?: Record<string, { target?: string; actions?: unknown }> }
      >;
    };
    const cfg = checkoutMachine.config as unknown as ConfigView;

    expect(cfg.initial).toBe("review");
    expect(FIRST_STEP_SLUG).toBe("review");

    expect(Object.keys(cfg.states)).toEqual([
      "review",
      "fulfilling",
      "done",
      "processing",
      "failed",
    ]);
    expect(Object.keys(cfg.states)).not.toContain("confirmSpend");

    expect(Object.keys(cfg.states.review.on ?? {})).toEqual(["CONFIRM"]);
    expect(cfg.states.review.on?.CONFIRM.target).toBe("fulfilling");
  });
});

describe("GUARD C \u{2014} every Buy handler navigates directly to express checkout (no cart hop)", () => {
  const HERE = dirname(fileURLToPath(import.meta.url));
  const APP = resolve(HERE, "../../../../..");

  const BUY_HANDLERS = [
    {
      file: "routes/app/routes/shop.tsx",
      fn: "onBuyAsset",
      path: "shop grid Buy now (2-click path)",
      maxNavs: 2,
    },
    {
      file: "features/src/components/marketplace/AssetDetailView.tsx",
      fn: "onBuy",
      path: "detail Buy (3-click path)",
      maxNavs: 1,
    },
  ] as const;

  function handlerBody(source: string, fn: string): string {
    const at = source.indexOf(`function ${fn}(`);
    if (at === -1) throw new Error(`buy handler ${fn} not found`);
    const open = source.indexOf("{", at);
    let depth = 0;
    for (let i = open; i < source.length; i++) {
      if (source[i] === "{") depth++;
      else if (source[i] === "}" && --depth === 0) return source.slice(open, i + 1);
    }
    throw new Error(`unbalanced braces in ${fn}`);
  }

  for (const h of BUY_HANDLERS) {
    it(`${h.path}: ${h.fn} \u{2192} /marketplace/checkout?express= with no cart step`, () => {
      const src = readFileSync(resolve(APP, h.file), "utf8");
      const body = handlerBody(src, h.fn);

      expect(body).toContain("/marketplace/checkout?express=");

      expect(body).not.toContain("/marketplace/cart");

      const navs = body.match(/navigate\(/g) ?? [];
      expect(navs.length).toBeGreaterThanOrEqual(1);
      expect(navs.length).toBeLessThanOrEqual(h.maxNavs);
    });
  }
});
