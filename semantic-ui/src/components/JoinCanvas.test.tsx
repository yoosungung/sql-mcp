import { describe, expect, it } from "vitest";
import { JOIN_FIT_VIEW } from "./JoinCanvas";

describe("JoinCanvas fit view", () => {
  it("caps initial auto-zoom at 1 so table text matches the UI font", () => {
    expect(JOIN_FIT_VIEW.maxZoom).toBe(1);
  });
});
