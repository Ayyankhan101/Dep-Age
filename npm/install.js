#!/usr/bin/env node
// postinstall: download the matching pre-built binary for this platform

const https = require("https");
const fs = require("fs");
const path = require("path");
const os = require("os");
const zlib = require("zlib");
const { execSync } = require("child_process");

const VERSION = require("./package.json").version;
const REPO = "Ayyankhan101/Dep-Age";
const BIN_NAME = os.platform() === "win32" ? "dep-age.exe" : "dep-age";
const BIN_DIR = path.join(__dirname, "bin");
const BIN_PATH = path.join(BIN_DIR, BIN_NAME);

function getAssetName() {
  const platform = os.platform();
  const arch = os.arch();

  if (platform === "linux" && arch === "x64")
    return `dep-age-v${VERSION}-x86_64-unknown-linux-gnu.tar.gz`;
  if (platform === "linux" && arch === "arm64")
    return `dep-age-v${VERSION}-aarch64-unknown-linux-gnu.tar.gz`;
  if (platform === "darwin" && arch === "x64")
    return `dep-age-v${VERSION}-x86_64-apple-darwin.tar.gz`;
  if (platform === "darwin" && arch === "arm64")
    return `dep-age-v${VERSION}-aarch64-apple-darwin.tar.gz`;
  if (platform === "win32" && arch === "x64")
    return `dep-age-v${VERSION}-x86_64-pc-windows-msvc.zip`;

  return null;
}

function download(url, dest) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(dest);
    https
      .get(url, { headers: { "User-Agent": "dep-age-npm-installer" } }, (res) => {
        if (res.statusCode === 302 || res.statusCode === 301) {
          // Follow redirect
          https.get(res.headers.location, { headers: { "User-Agent": "dep-age-npm-installer" } }, (res2) => {
            res2.pipe(file);
            file.on("finish", () => { file.close(); resolve(); });
          }).on("error", reject);
          return;
        }
        res.pipe(file);
        file.on("finish", () => { file.close(); resolve(); });
      })
      .on("error", reject);
  });
}

async function main() {
  const assetName = getAssetName();
  if (!assetName) {
    console.error(
      `dep-age: Unsupported platform ${os.platform()}/${os.arch()}. ` +
        `Please install Rust and run: cargo install dep-age`
    );
    process.exit(1);
  }

  // Skip if binary already exists
  if (fs.existsSync(BIN_PATH)) {
    return;
  }

  const downloadUrl = `https://github.com/${REPO}/releases/download/v${VERSION}/${assetName}`;
  const tmpDir = os.tmpdir();
  const archivePath = path.join(tmpDir, assetName);

  console.log(`dep-age: Downloading ${assetName}...`);

  try {
    fs.mkdirSync(BIN_DIR, { recursive: true });
    await download(downloadUrl, archivePath);

    if (assetName.endsWith(".tar.gz")) {
      const tar = require("child_process").execFileSync;
      // Extract tar.gz into bin/
      require("child_process").execSync(`tar xzf "${archivePath}" -C "${BIN_DIR}"`, { stdio: "pipe" });
    } else if (assetName.endsWith(".zip")) {
      // Windows: extract .exe from zip using system unzip or node unzip
      const AdmZip = (() => {
        try { return require("adm-zip"); } catch { return null; }
      })();
      if (AdmZip) {
        const zip = new AdmZip(archivePath);
        zip.extractAllTo(BIN_DIR, true);
      } else {
        // Fallback: use PowerShell on Windows
        require("child_process").execSync(
          `powershell -command "Expand-Archive -Path '${archivePath}' -DestinationPath '${BIN_DIR}' -Force"`,
          { stdio: "pipe" }
        );
      }
    }

    // Make executable on unix
    if (os.platform() !== "win32") {
      fs.chmodSync(BIN_PATH, 0o755);
    }

    // Cleanup
    fs.unlinkSync(archivePath);

    console.log(`dep-age: Installed to ${BIN_PATH}`);
  } catch (err) {
    // Cleanup on error
    try { fs.unlinkSync(archivePath); } catch {}
    console.error(`dep-age: Failed to download binary: ${err.message}`);
    console.error("Install Rust and run: cargo install dep-age");
    process.exit(1);
  }
}

main();
