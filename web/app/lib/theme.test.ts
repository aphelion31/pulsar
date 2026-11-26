import { describe, expect, test } from "bun:test";
import { getTheme, isDarkEffective } from "./theme";

describe("theme helpers", () => {
  test("returns explicit light and dark decisions cleanly without window", () => {
    expect(getTheme()).toBe("light");
    expect(isDarkEffective("dark")).toBe(true);
    expect(isDarkEffective("light")).toBe(false);
    expect(isDarkEffective("auto")).toBe(false);
  });
});
