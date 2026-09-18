import { expect, test } from "vitest";
import { loader } from "./events";

test("legacy public events links keep their filters when redirected", () => {
  const response = loader({ request: new Request("https://catalyst.example.com/events/?search=music&category=art") });
  expect(response.status).toBe(302);
  expect(response.headers.get("Location")).toBe("/whats-on?search=music&category=art");
});
