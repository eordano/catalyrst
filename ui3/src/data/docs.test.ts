import { afterEach, describe, expect, test } from "vitest";
import { DOCS_BASE, docsBase, docsUrl } from "./docs";

const env = (globalThis as { process: { env: Record<string, string | undefined> } }).process.env;

afterEach(() => {
  delete (window as { __DCL_PUBLIC__?: unknown }).__DCL_PUBLIC__;
  delete env.DOCS_BASE;
});

describe("docsUrl", () => {
  test("defaults to the self-hosted mirror's directory urls, keeping fragments after the trailing slash", () => {
    expect(docsBase()).toBe(DOCS_BASE);
    expect(docsUrl()).toBe("/docs/");
    expect(docsUrl("")).toBe("/docs/");
    expect(docsUrl("/")).toBe("/docs/");
    expect(docsUrl("creator")).toBe("/docs/creator/");
    expect(docsUrl("/creator/")).toBe("/docs/creator/");
    expect(docsUrl("creator/sdk7/getting-started/sdk-101")).toBe(
      "/docs/creator/sdk7/getting-started/sdk-101/",
    );
    expect(docsUrl("creator/sdk7/projects/scene-metadata#required-permissions")).toBe(
      "/docs/creator/sdk7/projects/scene-metadata/#required-permissions",
    );
    expect(docsUrl("player")).not.toMatch(/decentraland\.org/);
  });

  test("the injected SSR global rebases every link, absolute hosts allowed, trailing slashes normalised", () => {
    window.__DCL_PUBLIC__ = { docsBase: "https://docs.decentraland.org/" };
    expect(docsBase()).toBe("https://docs.decentraland.org");
    expect(docsUrl()).toBe("https://docs.decentraland.org/");
    expect(docsUrl("/creator/")).toBe("https://docs.decentraland.org/creator/");
    expect(docsUrl("creator/sdk7/projects/scene-metadata#required-permissions")).toBe(
      "https://docs.decentraland.org/creator/sdk7/projects/scene-metadata/#required-permissions",
    );

    window.__DCL_PUBLIC__ = { docsBase: "/handbook" };
    expect(docsUrl("player")).toBe("/handbook/player/");

    window.__DCL_PUBLIC__ = { docsBase: "/" };
    expect(docsUrl()).toBe("/");
    expect(docsUrl("player")).toBe("/player/");

    window.__DCL_PUBLIC__ = { docsBase: "" };
    expect(docsUrl("player")).toBe("/docs/player/");
  });

  test("the server reads DOCS_BASE from the process env so SSR markup matches the hydrated client", () => {
    env.DOCS_BASE = "https://docs.decentraland.org";
    expect(docsUrl("creator")).toBe("https://docs.decentraland.org/creator/");

    window.__DCL_PUBLIC__ = { docsBase: "/mirror/" };
    expect(docsUrl("creator")).toBe("/mirror/creator/");

    env.DOCS_BASE = "";
    delete (window as { __DCL_PUBLIC__?: unknown }).__DCL_PUBLIC__;
    expect(docsUrl("creator")).toBe("/docs/creator/");
  });
});
