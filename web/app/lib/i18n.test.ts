import { describe, expect, test } from "bun:test";
import { getLocale, isRtlLocale, SUPPORTED_LOCALES, t } from "./i18n";

describe("i18n", () => {
  test("interpolates variables and preserves missing keys safely", () => {
    expect(t("REPORT.HEADER")).toBe("Conversations");
    expect(t("YEAR_IN_REVIEW.TITLE")).toBe("Year in Review");
    expect(t("YEAR_IN_REVIEW.SHARE_MODAL.SHARE_TITLE", { year: 2026 })).toBe("My 2026 Year in Review");
    expect(t("NONEXISTENT.KEY")).toBe("NONEXISTENT.KEY");
  });

  test("includes all 57 Chatwoot locales with 5 RTL languages", () => {
    const codes = SUPPORTED_LOCALES.map((l) => l.code);
    expect(codes.length).toBe(57);
    expect(codes[0]).toBe("en");
    for (const expected of ["id", "es", "fr", "de", "pt_BR", "ja", "zh_CN", "zh_TW", "ar", "fa", "he", "ur", "ur_IN"]) {
      expect(codes).toContain(expected);
    }
    for (const rtl of ["ar", "fa", "he", "ur", "ur_IN"]) {
      expect(isRtlLocale(rtl)).toBe(true);
    }
    expect(isRtlLocale("id")).toBe(false);
    expect(isRtlLocale("en")).toBe(false);
    expect(getLocale()).toBe("en");
  });
});
