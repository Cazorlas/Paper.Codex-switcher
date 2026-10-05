#!/usr/bin/env node
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");
const exe = path.join(__dirname, process.platform === "win32" ? "paper-codex-switch.exe" : "paper-codex-switch");
// npm may skip the postinstall script (allow-scripts); fetch the binary on first run instead.
if (!fs.existsSync(exe)) {
  const r = spawnSync(process.execPath, [path.join(__dirname, "..", "install.js")], { stdio: "inherit" });
  if (r.status !== 0 || !fs.existsSync(exe)) {
    console.error("paper-codex-switch: could not download the binary; see https://github.com/Cazorlas/Paper.Codex-switcher/releases");
    process.exit(1);
  }
}
const r = spawnSync(exe, process.argv.slice(2), { stdio: "inherit" });
if (r.error) {
  console.error(`paper-codex-switch: cannot run ${exe}: ${r.error.message}`);
  process.exit(1);
}
process.exit(r.status ?? 1);
