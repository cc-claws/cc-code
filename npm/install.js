#!/usr/bin/env node

const { createWriteStream, mkdirSync, chmodSync, existsSync } = require("fs");
const { join } = require("path");
const { get } = require("https");
const { execSync } = require("child_process");

const VERSION = require("./package.json").version;
const REPO = "wismyzhizi2018/peri";
const BASE_URL = `https://github.com/${REPO}/releases/download/npm-v${VERSION}`;

const PLATFORMS = {
  "linux-x64": { os: "linux", arch: "x64", suffix: "linux-x86_64", ext: "tar.gz" },
  "linux-arm64": { os: "linux", arch: "arm64", suffix: "linux-aarch64", ext: "tar.gz" },
  "darwin-x64": { os: "darwin", arch: "x64", suffix: "macos-x86_64", ext: "tar.gz" },
  "darwin-arm64": { os: "darwin", arch: "arm64", suffix: "macos-aarch64", ext: "tar.gz" },
  "win32-x64": { os: "win32", arch: "x64", suffix: "windows-x86_64", ext: "zip" },
};

function getPlatformKey() {
  const os = process.platform;
  const arch = process.arch;
  const key = `${os}-${arch}`;
  if (!PLATFORMS[key]) {
    throw new Error(`Unsupported platform: ${os}-${arch}. Supported: ${Object.keys(PLATFORMS).join(", ")}`);
  }
  return key;
}

function download(url) {
  return new Promise((resolve, reject) => {
    get(url, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        download(res.headers.location).then(resolve, reject);
        return;
      }
      if (res.statusCode !== 200) {
        reject(new Error(`Download failed: HTTP ${res.statusCode} for ${url}`));
        return;
      }
      const chunks = [];
      res.on("data", (chunk) => chunks.push(chunk));
      res.on("end", () => resolve(Buffer.concat(chunks)));
      res.on("error", reject);
    }).on("error", reject);
  });
}

function extractTarGz(buffer, dest) {
  const { writeFileSync, unlinkSync } = require("fs");
  const tmpFile = join(dest, "peri.tar.gz");
  writeFileSync(tmpFile, buffer);
  execSync(`tar -xzf "${tmpFile}" -C "${dest}"`, { stdio: "ignore" });
  unlinkSync(tmpFile);
}

function extractZip(buffer, dest) {
  const { writeFileSync } = require("fs");
  const AdmZip = require("adm-zip");
  const zip = new AdmZip(buffer);
  zip.extractAllTo(dest, true);
}

async function main() {
  const key = getPlatformKey();
  const platform = PLATFORMS[key];
  const fileName = `peri-${platform.suffix}.${platform.ext}`;
  const url = `${BASE_URL}/${fileName}`;
  const binDir = join(__dirname, "bin");

  if (!existsSync(binDir)) {
    mkdirSync(binDir, { recursive: true });
  }

  console.log(`Downloading peri ${VERSION} for ${platform.os}-${platform.arch}...`);
  console.log(`  URL: ${url}`);

  const buffer = await download(url);

  if (platform.ext === "tar.gz") {
    extractTarGz(buffer, binDir);
  } else {
    extractZip(buffer, binDir);
  }

  // Rename binary and set permissions
  const binaryName = platform.os === "win32" ? "peri.exe" : "peri-bin";
  const binaryPath = join(binDir, binaryName);

  if (platform.os !== "win32") {
    chmodSync(binaryPath, 0o755);
  }

  console.log(`peri ${VERSION} installed successfully.`);
}

main().catch((err) => {
  console.error("Failed to install peri:", err.message);
  process.exit(1);
});
