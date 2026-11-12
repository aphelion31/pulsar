export type Vars = Record<string, string | number>;

/// vue-i18n plural forms: "one | many" or "zero | one | many", chosen by `vars.count` like `t(key, n)`.
/// Without a count the last (plural) form is used.
export function pickPlural(raw: string, vars?: Vars): string {
  const forms = raw.split(/\s\|\s/);
  if (forms.length < 2) return raw;
  const n = vars && "count" in vars ? Number(vars.count) : 2;
  if (forms.length === 2) return n === 1 ? forms[0] : forms[1];
  return forms[Math.min(n, 2)];
}
