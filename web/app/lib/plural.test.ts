import { describe, expect, test } from "bun:test";
import { pickPlural } from "./plural";

describe("pickPlural", () => {
  test("selects two-form singular and plural based on count", () => {
    expect(pickPlural("1 conversation | {count} conversations", { count: 1 })).toBe("1 conversation");
    expect(pickPlural("1 conversation | {count} conversations", { count: 5 })).toBe("{count} conversations");
  });

  test("selects three-form zero/one/many", () => {
    const template = "No items | One item | {count} items";
    expect(pickPlural(template, { count: 0 })).toBe("No items");
    expect(pickPlural(template, { count: 1 })).toBe("One item");
    expect(pickPlural(template, { count: 4 })).toBe("{count} items");
  });
});
