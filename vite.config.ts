/// <reference types="vitest/config" />
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";
import product from "./product.json";

/** Fills %DISPLAY_NAME% in index.html from product.json (the single source of naming). */
const productHtml = (): Plugin => ({
  name: "product-html",
  transformIndexHtml: (html) => html.replaceAll("%DISPLAY_NAME%", product.displayName),
});

// The UI lives in ./ui and is built into ./dist, which the Tauri app embeds
// (crates/app/tauri.conf.json → build.frontendDist).
export default defineConfig({
  root: "ui",
  plugins: [react(), tailwindcss(), productHtml()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/crates/**", "**/target/**"] } },
  build: { outDir: "../dist", emptyOutDir: true, target: "es2022", sourcemap: false },
  test: {
    root: ".",
    environment: "jsdom",
    setupFiles: ["ui/src/test/setup.ts"],
    include: ["ui/src/**/*.test.{ts,tsx}"],
    css: false,
  },
});
