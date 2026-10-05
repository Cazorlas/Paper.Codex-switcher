#!/usr/bin/env node
const path = require("path");
const { spawnSync } = require("child_process");
const exe = path.join(__dirname, process.platform === "win32" ? "paper-codex-switch.exe" : "paper-codex-switch");
const r = spawnSync(exe, process.argv.slice(2), { stdio: "inherit" });
if (r.error) {
  console.error(`paper-codex-switch: cannot run ${exe}: ${r.error.message}\nTry reinstalling: npm i -g paper-codex-switch`);
  process.exit(1);
}
process.exit(r.status ?? 1);
