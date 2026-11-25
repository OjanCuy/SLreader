// Downloads the PDFium library that SLReader uses to read text from PDFs (T-14).
// Puts pdfium.dll and its licenses in src-tauri/pdfium/ (not committed to git).
//
// The download is checked against a fixed SHA-256 so a changed file is never used.
// The version must match the `pdfium_8125` feature of pdfium-render in src-tauri/Cargo.toml.
//
// Usage: npm run fetch-pdfium

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const VERSION = "8125";
const URL = `https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F${VERSION}/pdfium-win-x64.tgz`;
const ARCHIVE_SHA256 = "9a964261760cff52570aabf038ab346d3a5034dd8b30d0d5fe8861235d356b8b";
const DLL_SHA256 = "4b0dec92555d69437bef0df37be630181ab3e83a6823ca264ede355a4f2ef994";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const target = join(root, "src-tauri", "pdfium");
const dll = join(target, "pdfium.dll");

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

if (existsSync(dll) && sha256(readFileSync(dll)) === DLL_SHA256) {
  console.log(`PDFium ${VERSION} is already in src-tauri/pdfium.`);
  process.exit(0);
}

console.log(`Downloading PDFium ${VERSION}...`);
const response = await fetch(URL);
if (!response.ok) {
  throw new Error(`Download failed: HTTP ${response.status} for ${URL}`);
}
const archive = Buffer.from(await response.arrayBuffer());
if (sha256(archive) !== ARCHIVE_SHA256) {
  throw new Error("The downloaded PDFium archive does not match the expected SHA-256. Not using it.");
}

const work = mkdtempSync(join(tmpdir(), "slreader-pdfium-"));
try {
  const file = join(work, "pdfium.tgz");
  writeFileSync(file, archive);
  // tar is built into Windows 10 and later.
  execFileSync("tar", ["-xzf", "pdfium.tgz"], { cwd: work, stdio: "inherit" });
  const extracted = join(work, "bin", "pdfium.dll");
  if (sha256(readFileSync(extracted)) !== DLL_SHA256) {
    throw new Error("pdfium.dll in the archive does not match the expected SHA-256. Not using it.");
  }

  rmSync(target, { recursive: true, force: true });
  mkdirSync(join(target, "licenses"), { recursive: true });
  cpSync(extracted, dll);
  // PDFium and the libraries inside it are open source; their licenses ship with the app.
  cpSync(join(work, "licenses"), join(target, "licenses"), { recursive: true });
  cpSync(join(work, "LICENSE"), join(target, "licenses", "pdfium-binaries.txt"));
  console.log(`PDFium ${VERSION} is ready in src-tauri/pdfium.`);
} finally {
  rmSync(work, { recursive: true, force: true });
}
