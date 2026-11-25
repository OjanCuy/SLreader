import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";
import { defineConfig } from "vitest/config";

// Settings for running inside Tauri. See https://tauri.app/start/frontend/vite/
export default defineConfig({
  // svelteTesting() only acts under Vitest: it picks Svelte's browser build.
  plugins: [svelte(), svelteTesting()],
  // Keep Rust errors visible in the terminal.
  clearScreen: false,
  server: {
    // Tauri expects this exact port (see devUrl in src-tauri/tauri.conf.json).
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    // WebView2 on Windows is Chromium-based.
    // 123 is the first version with the CSS light-dark() function used by the theme tokens.
    target: "chrome123",
    outDir: "dist",
    // Keep fonts as files: inlined data: URLs would be blocked by the CSP (font-src 'self').
    assetsInlineLimit: 0,
  },
  test: {
    include: ["src/**/*.test.ts"],
  },
});
