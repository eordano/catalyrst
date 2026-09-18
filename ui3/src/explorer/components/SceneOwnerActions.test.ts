import { describe, expect, it } from "vitest";

import { FEEDBACK_MAX, composeFeedback, normalizeParcel } from "./SceneOwnerActions";

describe("normalizeParcel", () => {
  it("canonicalizes spacing and signs and rejects anything that is not a parcel pair", () => {
    expect(normalizeParcel("-143,102")).toBe("-143,102");
    expect(normalizeParcel(" 7 , -9 ")).toBe("7,-9");
    expect(normalizeParcel("-0,0")).toBe("0,0");
    expect(normalizeParcel("")).toBeNull();
    expect(normalizeParcel("1,2,3")).toBeNull();
    expect(normalizeParcel("x,y")).toBeNull();
    expect(normalizeParcel(undefined)).toBeNull();
  });
});

describe("composeFeedback", () => {
  it("prefixes the scene and parcel and never exceeds the social service message cap", () => {
    expect(composeFeedback("CBD Plaza", "-143,102", "  the door is stuck ")).toBe(
      "Feedback on CBD Plaza (-143,102): the door is stuck",
    );
    expect(composeFeedback("", "0,0", "hi")).toBe("Feedback on your scene (0,0): hi");
    const long = "x".repeat(FEEDBACK_MAX * 2);
    expect(composeFeedback("Scene", "1,1", long).length).toBe(FEEDBACK_MAX);
  });
});
