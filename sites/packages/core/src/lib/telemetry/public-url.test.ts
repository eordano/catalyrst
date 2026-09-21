import { expect, it } from "vitest";
import { publicTelemetryUrl } from "./public-url";

it("keeps server loopback addresses out of browser configuration", () => {
  for (const url of ["http://127.0.0.1:5150", "http://localhost:5150", "http://[::1]:5150", "http://0.0.0.0:5150"]) {
    expect(publicTelemetryUrl(url)).toBe("/telemetry");
  }
  expect(publicTelemetryUrl("https://telemetry.example.com")).toBe("https://telemetry.example.com");
  expect(publicTelemetryUrl("/telemetry")).toBe("/telemetry");
  expect(publicTelemetryUrl(undefined)).toBe("");
  expect(publicTelemetryUrl("invalid")).toBe("");
});
