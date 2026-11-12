// node --experimental-strip-types tools/plural-check.ts
import { pickPlural } from "../web/app/lib/plural.ts";
const eq = (got: string, want: string) => {
  if (got !== want) throw new Error(`got ${got}, want ${want}`);
};
eq(pickPlural("1 item | {n} items", { count: 1 }), "1 item");
eq(pickPlural("1 item | {n} items", { count: 5 }), "{n} items");
eq(pickPlural("none | one | many", { count: 0 }), "none");
eq(pickPlural("none | one | many", { count: 7 }), "many");
eq(pickPlural("a | b", {}), "b");
eq(pickPlural("plain text", { count: 1 }), "plain text");
console.log("plural ok");
