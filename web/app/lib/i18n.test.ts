import { describe, expect, test } from "bun:test";
import { getLocale, isRtlLocale, SUPPORTED_LOCALES, t } from "./i18n";

describe("i18n", () => {
  test("interpolates variables and preserves missing keys safely", () => {
    expect(t("REPORT.HEADER")).toBe("Conversations");
    expect(t("YEAR_IN_REVIEW.TITLE")).toBe("Year in Review");
    expect(t("YEAR_IN_REVIEW.SHARE_MODAL.SHARE_TITLE", { year: 2026 })).toBe("My 2026 Year in Review");
    expect(t("NONEXISTENT.KEY")).toBe("NONEXISTENT.KEY");
  });

  test("includes all 9 supported locales with Arabic RTL flag", () => {
    expect(SUPPORTED_LOCALES.map((l) => l.code)).toEqual([
      "en",
      "id",
      "es",
      "fr",
      "de",
      "pt_BR",
      "ja",
      "zh_CN",
      "ar",
    ]);
    expect(isRtlLocale("ar")).toBe(true);
    expect(isRtlLocale("id")).toBe(false);
    expect(getLocale()).toBe("en");
  });
});
