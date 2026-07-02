#!/usr/bin/env node
// Entry point for `dep-age` when installed via npm
const { spawnSync } = require("child_process");
const path = require("path");
const os = require("os");
const fs = require("fs");

const BIN_NAME = os.platform() === "win32" ? "dep-age.exe" : "dep-age";
const BIN_PATH = path.join(__dirname, "bin", BIN_NAME);

if (!fs.existsSync(BIN_PATH)) {
  console.error("dep-age: Binary not found. Run `npm install dep-age` or `cargo install dep-age`.");
  process.exit(1);
}

try {
  const result = spawnSync(BIN_PATH, process.argv.slice(2), { stdio: "inherit" });
  process.exit(result.status || 0);
} catch (err) {
  console.error(`dep-age: Failed to run binary: ${err.message}`);
  process.exit(1);
}
