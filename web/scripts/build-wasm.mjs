import { spawnSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const webDir = dirname(fileURLToPath(import.meta.url));
const root = resolve(webDir, "..", "..");
const outDir = resolve(root, "web", "src", "wasm");
mkdirSync(outDir, { recursive: true });

function run(cmd, args) {
  const r = spawnSync(cmd, args, { cwd: root, stdio: "inherit", shell: true });
  if (r.status !== 0) {
    process.exit(r.status ?? 1);
  }
}

run("cargo", [
  "build",
  "-p",
  "pii-wasm",
  "--target",
  "wasm32-unknown-unknown",
  "--release",
]);
run("wasm-bindgen", [
  "--target",
  "web",
  "--out-dir",
  outDir,
  resolve(root, "target", "wasm32-unknown-unknown", "release", "pii_wasm.wasm"),
]);
