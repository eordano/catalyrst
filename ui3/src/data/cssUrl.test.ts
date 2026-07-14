import { describe, expect, test } from "vitest";

import { safeCssUrl } from "./cssUrl";

describe("safeCssUrl", () => {
  test("wraps a plain https url", () => {
    expect(safeCssUrl("https://peer.example/face.png")).toBe(
      'url("https://peer.example/face.png")',
    );
  });

  test("keeps http and re-serializes through the URL parser", () => {
    expect(safeCssUrl("http://peer.example:8080/a/../face.png")).toBe(
      'url("http://peer.example:8080/face.png")',
    );
  });

  test("rejects a hostile string that tries to break out of url()", () => {
    expect(
      safeCssUrl('https://x.example/a.png"); background: url(javascript:alert(1)'),
    ).toBeNull();
  });

  test("rejects non-http schemes", () => {
    expect(safeCssUrl("javascript:alert(1)")).toBeNull();
    expect(safeCssUrl("data:image/png;base64,AAAA")).toBeNull();
    expect(safeCssUrl("file:///etc/passwd")).toBeNull();
  });

  test("rejects urls that keep quotes or parens after serializing", () => {
    expect(safeCssUrl("https://x.example/a(b).png")).toBeNull();
    expect(safeCssUrl("https://x.example/a'b.png")).toBeNull();
  });

  test("rejects unparseable and empty input", () => {
    expect(safeCssUrl("not a url")).toBeNull();
    expect(safeCssUrl("")).toBeNull();
    expect(safeCssUrl(null)).toBeNull();
    expect(safeCssUrl(undefined)).toBeNull();
  });
});
