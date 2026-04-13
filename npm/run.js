#!/usr/bin/env node
// Entry point for `dep-age` when installed via npm
const { spawnSync } = require("child_process");
const path = require("path");
const os = require("os");

const BIN_NAME = os.platform() === "win32" ? "dep-age.exe" : "dep-age";
const BIN_PATH = path.join(__dirname, "bin", BIN_NAME);

const result = spawnSync(BIN_PATH, process.argv.slice(2), { stdio: "inherit" });
process.exit(result.status || 0);
