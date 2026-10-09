// Captures UI screenshots from the browser preview (sample data). Usage:
//   npx vite build && npx vite preview --port 4173 &  node scripts/screenshots.mjs <outDir> [light|dark]
import { chromium } from "@playwright/test";

const out = process.argv[2];
const theme = process.argv[3] ?? "light";
const browser = await chromium
  .launch()
  .catch(() => chromium.launch({ executablePath: "/opt/pw-browsers/chromium" }));
const page = await browser.newPage({
  viewport: { width: 1366, height: 860 },
  deviceScaleFactor: 1,
  colorScheme: theme,
});
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));
page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
await page.goto("http://localhost:4173/");
await page.waitForTimeout(500);
await page.screenshot({ path: `${out}/01-onboarding-${theme}.png` });
await page.getByRole("button", { name: "Next" }).click();
await page.getByRole("button", { name: "Next" }).click();
await page.waitForTimeout(400);
await page.screenshot({ path: `${out}/02-onboarding3-${theme}.png` });
await page.getByRole("button", { name: "Get started" }).click();
await page.waitForTimeout(300);
await page.screenshot({ path: `${out}/03-overview-${theme}.png` });
await page.getByRole("button", { name: "Scan", exact: true }).first().click();
await page.waitForTimeout(1200);
await page.screenshot({ path: `${out}/04-scanning-${theme}.png` });
await page.waitForTimeout(2200);
await page.screenshot({ path: `${out}/05-scanned-${theme}.png` });
await page.getByRole("main").getByRole("button", { name: "Explore", exact: true }).first().click();
await page.waitForTimeout(1500);
await page.screenshot({ path: `${out}/06-explore-${theme}.png` });
const map = page.getByRole("application");
const box = await map.boundingBox();
await page.mouse.move(box.x + box.width * 0.3, box.y + box.height * 0.4);
await page.waitForTimeout(400);
await page.screenshot({ path: `${out}/07-hover-${theme}.png` });
await page.mouse.click(box.x + box.width * 0.3, box.y + box.height * 0.4, { button: "right" });
await page.waitForTimeout(300);
await page.screenshot({ path: `${out}/08-menu-${theme}.png` });
await page.keyboard.press("Escape");
await page.mouse.click(box.x + box.width * 0.3, box.y + box.height * 0.4);
await page.waitForTimeout(1200);
await page.screenshot({ path: `${out}/09-zoomed-${theme}.png` });
await page.getByRole("tab", { name: "File types" }).click();
await page.getByRole("radio", { name: "Age" }).click();
await page.waitForTimeout(1000);
await page.screenshot({ path: `${out}/10-types-age-${theme}.png` });
await page.getByRole("button", { name: /Settings/ }).click();
await page.waitForTimeout(300);
await page.screenshot({ path: `${out}/11-settings-${theme}.png` });
console.log(JSON.stringify(errors));
await browser.close();
