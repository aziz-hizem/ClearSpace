#!/usr/bin/env node
// Applies product.json (the single source of product identity) to every file that needs
// it. `--check` only verifies and exits non-zero when something is out of date (used in CI).
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const product = JSON.parse(readFileSync(join(root, "product.json"), "utf8"));
const check = process.argv.includes("--check");

const problems = [];
if (!product.displayName || product.displayName.length > 40)
  problems.push("displayName must be 1–40 characters");
if (!/^[a-zA-Z][a-zA-Z0-9-]*(\.[a-zA-Z0-9-]+)+$/.test(product.bundleId ?? ""))
  problems.push("bundleId must be reverse-DNS (letters, digits, hyphens, dots)");
if (!/^[A-Za-z0-9._-]+$/.test(product.installerName ?? ""))
  problems.push("installerName may only use letters, digits, '.', '_' and '-'");
if (!/^\d+\.\d+\.\d+$/.test(product.version ?? "")) problems.push("version must be MAJOR.MINOR.PATCH");
if (problems.length) {
  console.error(`product.json is invalid:\n  ${problems.join("\n  ")}`);
  process.exit(1);
}

/** Files derived from product.json and how to update each one. */
const targets = [
  {
    file: "crates/app/tauri.conf.json",
    apply(conf) {
      conf.productName = product.displayName;
      conf.mainBinaryName = product.installerName;
      conf.version = product.version;
      conf.identifier = product.bundleId;
      conf.bundle.publisher = product.publisher;
      conf.bundle.copyright = `© ${product.publisher}`;
      for (const w of conf.app.windows) w.title = product.displayName;
      return conf;
    },
  },
  {
    file: "package.json",
    apply(pkg) {
      pkg.version = product.version;
      return pkg;
    },
  },
];

let stale = 0;
for (const t of targets) {
  const path = join(root, t.file);
  const before = readFileSync(path, "utf8");
  const after = `${JSON.stringify(t.apply(JSON.parse(before)), null, 2)}\n`;
  if (before !== after) {
    stale++;
    if (check) console.error(`${t.file} is out of sync with product.json (run: npm run product:sync)`);
    else {
      writeFileSync(path, after);
      console.log(`updated ${t.file}`);
    }
  }
}
if (check && stale) process.exit(1);
if (check) console.log("product.json: all derived files are in sync");
