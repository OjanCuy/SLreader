import { createReadStream, readdirSync, readFileSync, statSync } from "node:fs";
import { extname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";
import type { Plugin } from "vite";
import { defineConfig } from "vitest/config";

/**
 * Serves the helper files of PDF.js at /pdfjs/<folder>/ (T-16): character maps,
 * standard fonts, image decoders, and a color profile. They are fetched by the PDF.js
 * worker only when a PDF needs them. See src/reader/pdfjs.ts.
 */
function pdfjsAssets(): Plugin {
  const root = fileURLToPath(new URL("./node_modules/pdfjs-dist/", import.meta.url));
  const folders = ["cmaps", "standard_fonts", "wasm", "iccs"];
  const types: Record<string, string> = { ".wasm": "application/wasm", ".js": "text/javascript" };
  // QuickJS runs JavaScript inside PDFs. SLReader never runs it, so it is not shipped.
  const wanted = (name: string) => !name.startsWith(".") && !name.startsWith("quickjs");

  return {
    name: "slreader-pdfjs-assets",
    configureServer(server) {
      server.middlewares.use("/pdfjs", (request, response, next) => {
        const [folder, encoded, ...more] = (request.url ?? "").split("?")[0].split("/").filter(Boolean);
        let name = "";
        try {
          name = decodeURIComponent(encoded ?? "");
        } catch {
          return next();
        }
        // One plain file name inside one of the folders: nothing like "..%2F" gets out.
        if (!folders.includes(folder) || more.length > 0 || !name || /[\\/:]/.test(name) || !wanted(name)) {
          return next();
        }
        const file = join(root, folder, name);
        if (!statSync(file, { throwIfNoEntry: false })?.isFile()) return next();
        response.setHeader("Content-Type", types[extname(file)] ?? "application/octet-stream");
        createReadStream(file).pipe(response);
      });
    },
    generateBundle() {
      // PDF.js is Apache 2.0: its license ships with the app.
      this.emitFile({ type: "asset", fileName: "pdfjs/LICENSE", source: readFileSync(join(root, "LICENSE")) });
      for (const folder of folders) {
        for (const name of readdirSync(join(root, folder)).filter(wanted)) {
          this.emitFile({
            type: "asset",
            fileName: `pdfjs/${folder}/${name}`,
            source: readFileSync(join(root, folder, name)),
          });
        }
      }
    },
  };
}

// Settings for running inside Tauri. See https://tauri.app/start/frontend/vite/
export default defineConfig({
  // svelteTesting() only acts under Vitest: it picks Svelte's browser build.
  plugins: [svelte(), svelteTesting(), pdfjsAssets()],
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
