import { describe, expect, test } from "vitest";

import { safeCssUrl } from "./cssUrl";

describe("safeCssUrl", () => {
  test("wraps http(s) urls after re-serializing them through the URL parser", () => {
    expect(safeCssUrl("https://peer.example/face.png")).toBe(
      'url("https://peer.example/face.png")',
    );
    expect(safeCssUrl("http://peer.example:8080/a/../face.png")).toBe(
      'url("http://peer.example:8080/face.png")',
    );
  });

  test("rejects breakout attempts, non-http schemes, leftover quotes or parens, and unparseable or empty input", () => {
    for (const hostile of [
      'https://x.example/a.png"); background: url(javascript:alert(1)',
      "javascript:alert(1)",
      "data:image/png;base64,AAAA",
      "file:///etc/passwd",
      "https://x.example/a(b).png",
      "https://x.example/a'b.png",
      "not a url",
      "",
      null,
      undefined,
    ]) {
      expect(safeCssUrl(hostile), String(hostile)).toBeNull();
    }
  });
});
