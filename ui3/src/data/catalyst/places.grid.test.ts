import { describe, expect, it } from "vitest";

import { PARCEL_PCT, coordsToPercent, parcelRectPercent } from "./places";

const GRID_MIN = -170;
const GRID_SPAN = 340;

describe("parcelRectPercent", () => {
  it("puts parcel (x,y) on the cell whose west edge is x and north edge is y+1, origin just below-right of 50%, garbage at 0,0", () => {
    const cell = parcelRectPercent("-143,102");
    expect(cell.left).toBeCloseTo(((-143 - GRID_MIN) / GRID_SPAN) * 100, 10);
    expect(cell.top).toBeCloseTo(((GRID_MIN + GRID_SPAN - 102 - 1) / GRID_SPAN) * 100, 10);
    expect(cell.size).toBeCloseTo(100 / GRID_SPAN, 10);
    expect(cell.size).toBe(PARCEL_PCT);
    const origin = parcelRectPercent("0,0");
    expect(origin.left).toBeCloseTo(50, 10);
    expect(origin.top).toBeCloseTo(50 - PARCEL_PCT, 10);
    expect(parcelRectPercent("nope")).toEqual(origin);
    expect(parcelRectPercent(null)).toEqual(origin);
  });
});

describe("coordsToPercent", () => {
  it("is the center of the parcel cell everywhere, so the pin always sits inside the square drawn for the same parcel", () => {
    for (const coords of ["0,0", "-143,102", "120,-75", " 7 , 9 ", "-170,169", "169,-170", "-170,-170", "169,169"]) {
      const cell = parcelRectPercent(coords);
      const center = coordsToPercent(coords);
      expect(center.left).toBeCloseTo(cell.left + cell.size / 2, 10);
      expect(center.top).toBeCloseTo(cell.top + cell.size / 2, 10);
      expect(center.left).toBeGreaterThan(cell.left);
      expect(center.left).toBeLessThan(cell.left + cell.size);
      expect(center.top).toBeGreaterThan(cell.top);
      expect(center.top).toBeLessThan(cell.top + cell.size);
    }
    expect(coordsToPercent("-170,169")).toEqual({ left: PARCEL_PCT / 2, top: PARCEL_PCT / 2 });
    expect(coordsToPercent("169,-170").left).toBeCloseTo(100 - PARCEL_PCT / 2, 10);
    expect(coordsToPercent("169,-170").top).toBeCloseTo(100 - PARCEL_PCT / 2, 10);
  });
});
