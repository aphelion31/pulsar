#!/usr/bin/env bun
// Merge Chatwoot MIT dashboard locale modules (chatwoot/app/javascript/dashboard/i18n/locale/<loc>/*.json)
// into per-locale merged JSON bundles in web/public/i18n/<loc>.json for runtime loading in web/app/lib/i18n.ts.
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const pulsarRoot = resolve(here, "..");
const chatwootLocaleRoot = resolve(
  pulsarRoot,
  "../chatwoot/app/javascript/dashboard/i18n/locale",
);
const outDir = join(pulsarRoot, "web/public/i18n");
const LOCALES = ["id", "es", "fr", "de", "pt_BR", "ja", "zh_CN", "ar"];

mkdirSync(outDir, { recursive: true });

for (const loc of LOCALES) {
  const srcDir = join(chatwootLocaleRoot, loc);
  const files = readdirSync(srcDir)
    .filter((f) => f.endsWith(".json"))
    .sort();
  const merged = {};
  for (const file of files) {
    const raw = readFileSync(join(srcDir, file), "utf8");
    const parsed = JSON.parse(raw);
    Object.assign(merged, parsed);
  }
  const dest = join(outDir, `${loc}.json`);
  writeFileSync(dest, JSON.stringify(merged));
  console.log(`synced ${loc}: ${files.length} files -> ${dest}`);
}
