#!/usr/bin/env node

const { createWriteStream, mkdirSync, chmodSync, existsSync, renameSync, unlinkSync, writeFileSync } = require("fs");
const { join } = require("path");
const { execSync } = require("child_process");

const { homedir } = require("os");

const VERSION = require("./package.json").version;
const REPO = "cc-claws/cc-code";
const BASE_URL = `https://github.com/${REPO}/releases/download/npm-v${VERSION}`;
const RG_VERSION = "15.0.1";
const RG_BASE_URL = `https://github.com/microsoft/ripgrep-prebuilt/releases/download/v${RG_VERSION}`;

const PLATFORMS = {
  "linux-x64": { os: "linux", arch: "x64", suffix: "linux-x86_64", ext: "tar.gz", rgSuffix: "x86_64-unknown-linux-musl" },
  "linux-arm64": { os: "linux", arch: "arm64", suffix: "linux-aarch64", ext: "tar.gz", rgSuffix: "aarch64-unknown-linux-gnu" },
  "darwin-x64": { os: "darwin", arch: "x64", suffix: "macos-x86_64", ext: "tar.gz", rgSuffix: "x86_64-apple-darwin" },
  "darwin-arm64": { os: "darwin", arch: "arm64", suffix: "macos-aarch64", ext: "tar.gz", rgSuffix: "aarch64-apple-darwin" },
  "win32-x64": { os: "win32", arch: "x64", suffix: "windows-x86_64", ext: "zip", rgSuffix: "x86_64-pc-windows-msvc" },
};

function getPlatformKey() {
  const key = `${process.platform}-${process.arch}`;
  if (!PLATFORMS[key]) {
    throw new Error(`Unsupported platform: ${key}. Supported: ${Object.keys(PLATFORMS).join(", ")}`);
  }
  return key;
}

function getProxyUrl() {
  // Check common proxy environment variables
  const proxy = process.env.HTTPS_PROXY || process.env.https_proxy
    || process.env.HTTP_PROXY || process.env.http_proxy
    || process.env.ALL_PROXY || process.env.all_proxy;
  return proxy || null;
}

function download(url) {
  const proxyUrl = getProxyUrl();

  if (proxyUrl) {
    return downloadViaProxy(url, proxyUrl);
  }
  return downloadDirect(url);
}

function downloadDirect(url) {
  const { get } = require("https");
  return new Promise((resolve, reject) => {
    get(url, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        downloadDirect(res.headers.location).then(resolve, reject);
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

function downloadViaProxy(url, proxyUrl) {
  const { URL } = require("url");
  const target = new URL(url);
  const proxy = new URL(proxyUrl);

  const isHttps = proxy.protocol === "https:" || proxy.protocol === "HTTPS:";
  const proxyModule = isHttps ? require("https") : require("http");

  const proxyOpts = {
    hostname: proxy.hostname,
    port: proxy.port || (isHttps ? 443 : 80),
    path: url,
    method: "GET",
    headers: { "Host": target.hostname, "User-Agent": "cc-code-installer" },
  };

  // Support proxy auth
  if (proxy.username) {
    const auth = decodeURIComponent(`${proxy.username}:${proxy.password || ""}`);
    proxyOpts.headers["Proxy-Authorization"] = `Basic ${Buffer.from(auth).toString("base64")}`;
  }

  console.log(`  Using proxy: ${proxy.hostname}:${proxy.port || (isHttps ? 443 : 80)}`);

  return new Promise((resolve, reject) => {
    const req = proxyModule.request(proxyOpts, (res) => {
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
    });
    req.on("error", reject);
    req.end();
  });
}

function extractTarGz(buffer, dest) {
  const tmpFile = join(dest, "cc-code.tar.gz");
  writeFileSync(tmpFile, buffer);
  execSync(`tar -xzf "${tmpFile}" -C "${dest}"`, { stdio: "ignore" });
  unlinkSync(tmpFile);
}

function extractZip(buffer, dest) {
  const AdmZip = require("adm-zip");
  const zip = new AdmZip(buffer);
  zip.extractAllTo(dest, true);
}

function migrateFromClaudeCode(home = homedir()) {
  const claudeSettingsPath = join(home, ".claude", "settings.json");
  const ccCodeDir = join(home, ".cc-code");
  const ccCodeSettingsPath = join(ccCodeDir, "settings.json");

  // 已有 cc-code 配置，跳过
  if (existsSync(ccCodeSettingsPath)) {
    return true;
  }

  // 无 Claude Code 配置
  if (!existsSync(claudeSettingsPath)) {
    return false;
  }

  let claudeSettings;
  try {
    claudeSettings = JSON.parse(require("fs").readFileSync(claudeSettingsPath, "utf-8"));
  } catch {
    return false;
  }

  const env = claudeSettings.env || {};
  const providers = [];

  // 检测 Anthropic
  const anthropicKey = env.ANTHROPIC_API_KEY || env.ANTHROPIC_AUTH_TOKEN || "";
  const anthropicBaseUrl = env.ANTHROPIC_BASE_URL || "";
  if (anthropicKey || anthropicBaseUrl) {
    const models = {};
    if (env.ANTHROPIC_DEFAULT_OPUS_MODEL) models.opus = env.ANTHROPIC_DEFAULT_OPUS_MODEL;
    if (env.ANTHROPIC_DEFAULT_SONNET_MODEL) models.sonnet = env.ANTHROPIC_DEFAULT_SONNET_MODEL;
    if (env.ANTHROPIC_DEFAULT_HAIKU_MODEL) models.haiku = env.ANTHROPIC_DEFAULT_HAIKU_MODEL;
    const p = {
      id: "anthropic",
      type: "anthropic",
      apiKey: anthropicKey,
    };
    if (anthropicBaseUrl) p.baseUrl = anthropicBaseUrl;
    if (Object.keys(models).length > 0) p.models = models;
    providers.push(p);
  }

  // 检测 OpenAI 兼容
  const openaiKey = env.OPENAI_API_KEY || env.CODEX_API_KEY || "";
  const openaiBaseUrl = env.OPENAI_BASE_URL || env.OPENAI_API_BASE || "";
  if (openaiKey || openaiBaseUrl) {
    const models = {};
    if (env.OPENAI_MODEL) models.sonnet = env.OPENAI_MODEL;
    const p = {
      id: "openai",
      type: "openai",
      apiKey: openaiKey,
    };
    if (openaiBaseUrl) p.baseUrl = openaiBaseUrl;
    if (Object.keys(models).length > 0) p.models = models;
    providers.push(p);
  }

  if (providers.length === 0) {
    return false;
  }

  if (!existsSync(ccCodeDir)) {
    mkdirSync(ccCodeDir, { recursive: true });
  }

  // 根据第一个 provider 的可用模型决定默认激活别名
  const firstProvider = providers[0];
  let activeAlias = "opus";
  if (firstProvider.models) {
    if (firstProvider.models.opus) activeAlias = "opus";
    else if (firstProvider.models.sonnet) activeAlias = "sonnet";
    else if (firstProvider.models.haiku) activeAlias = "haiku";
  }

  const ccCodeSettings = {
    config: {
      active_alias: activeAlias,
      active_provider_id: firstProvider.id,
      providers,
    },
  };
  writeFileSync(ccCodeSettingsPath, JSON.stringify(ccCodeSettings, null, 2) + "\n");
  console.log("");
  console.log("  Migrated ~/.claude/settings.json -> ~/.cc-code/settings.json");
  console.log(`  Found ${providers.length} provider(s): ${providers.map(p => p.type).join(", ")}`);
  return true;
}

async function downloadRipgrep(platform, binDir) {
  const rgName = platform.os === "win32" ? "rg.exe" : "rg";
  const rgPath = join(binDir, rgName);

  // 已存在则跳过
  if (existsSync(rgPath)) {
    console.log("  ripgrep already installed.");
    return;
  }

  try {
    const rgExt = platform.os === "win32" ? "zip" : "tar.gz";
    const rgFileName = `ripgrep-v${RG_VERSION}-${platform.rgSuffix}.${rgExt}`;
    const rgUrl = `${RG_BASE_URL}/${rgFileName}`;

    console.log(`  Downloading ripgrep ${RG_VERSION}...`);
    const rgBuffer = await download(rgUrl);

    // 解压 rg 二进制到 binDir
    if (rgExt === "tar.gz") {
      const tmpFile = join(binDir, "rg-download.tar.gz");
      writeFileSync(tmpFile, rgBuffer);
      // ripgrep tar.gz 内含目录 ripgrep-x.y.z-suffix/rg
      execSync(`tar -xzf "${tmpFile}" -C "${binDir}" --strip-components=1 --wildcards "*/rg" 2>/dev/null || tar -xzf "${tmpFile}" -C "${binDir}" --strip-components=1 "ripgrep-v${RG_VERSION}-${platform.rgSuffix}/rg"`, { stdio: "ignore" });
      unlinkSync(tmpFile);
    } else {
      const AdmZip = require("adm-zip");
      const zip = new AdmZip(rgBuffer);
      // Windows zip 内含 rg.exe 在根目录或子目录
      const entries = zip.getEntries();
      for (const entry of entries) {
        if (entry.entryName.endsWith("rg.exe")) {
          writeFileSync(rgPath, entry.getData());
          break;
        }
      }
    }

    if (existsSync(rgPath)) {
      if (platform.os !== "win32") chmodSync(rgPath, 0o755);
      console.log("  ripgrep installed (enhances Grep/Glob performance).");
    } else {
      console.log("  ripgrep extraction skipped (Grep/Glob will use built-in engine).");
    }
  } catch (err) {
    // rg 下载失败不阻塞安装，Rust 内置引擎作为 fallback
    console.log(`  ripgrep download skipped: ${err.message} (Grep/Glob will use built-in engine).`);
  }
}

async function main() {
  const key = getPlatformKey();
  const platform = PLATFORMS[key];
  const fileName = `cc-code-${platform.suffix}.${platform.ext}`;
  const url = `${BASE_URL}/${fileName}`;
  const binDir = join(__dirname, "bin");

  if (!existsSync(binDir)) {
    mkdirSync(binDir, { recursive: true });
  }

  console.log(`Downloading cc-code ${VERSION} for ${platform.os}-${platform.arch}...`);
  console.log(`  URL: ${url}`);

  const buffer = await download(url);

  if (platform.ext === "tar.gz") {
    extractTarGz(buffer, binDir);
  } else {
    extractZip(buffer, binDir);
  }

  const extractedName = platform.os === "win32"
    ? `cc-code-${platform.suffix}.exe`
    : `cc-code-${platform.suffix}`;
  const finalName = platform.os === "win32" ? "cc-code.exe" : "cc-code-bin";
  const extractedPath = join(binDir, extractedName);
  const finalPath = join(binDir, finalName);

  if (existsSync(extractedPath)) {
    if (existsSync(finalPath)) unlinkSync(finalPath);
    renameSync(extractedPath, finalPath);
  }

  if (platform.os !== "win32") {
    chmodSync(finalPath, 0o755);
    const wrapperPath = join(__dirname, "bin", "cc-code");
    if (existsSync(wrapperPath)) chmodSync(wrapperPath, 0o755);
  } else {
    // Generate Windows batch wrapper so npm's cc-code.cmd/ps1 can invoke it
    const binDirPath = join(__dirname, "bin");
    writeFileSync(join(binDirPath, "cc-code.cmd"), `@echo off\r\n"%~dp0cc-code.exe" %*\r\n`);
    writeFileSync(join(binDirPath, "cc-code.ps1"), `$basedir = Split-Path $MyInvocation.MyCommand.Definition -Parent\r\n& "$basedir\\cc-code.exe" @args\r\nexit $LASTEXITCODE\r\n`);
  }

  console.log(`cc-code ${VERSION} installed successfully.`);

  // 下载 ripgrep 预编译二进制（增强 Grep/Glob 性能，失败不阻塞安装）
  await downloadRipgrep(platform, binDir);

  const migrated = migrateFromClaudeCode();

  if (!migrated) {
    console.log("");
    console.log("─── Quick Start ───");
    console.log("");
    console.log("  Set your API key (pick one):");
    console.log("");
    console.log("     # DeepSeek");
    console.log("     export OPENAI_API_KEY=sk-xxx");
    console.log("     export OPENAI_BASE_URL=https://api.deepseek.com/v1");
    console.log("     export OPENAI_MODEL=deepseek-chat");
    console.log("");
    console.log("     # Anthropic");
    console.log("     export ANTHROPIC_API_KEY=sk-ant-xxx");
    console.log("");
    console.log("  Or create config file:");
    console.log("");
    console.log("     mkdir -p ~/.cc-code");
    console.log('     cat > ~/.cc-code/settings.json << \'EOF\'');
    console.log("     {");
    console.log('       "config": {');
    console.log('         "providers": [');
    console.log("           {");
    console.log('             "type": "openai",');
    console.log('             "apiKey": "sk-xxx",');
    console.log('             "baseUrl": "https://api.deepseek.com/v1",');
    console.log('             "models": { "sonnet": "deepseek-chat" }');
    console.log("           }");
    console.log("         ]");
    console.log("       }");
    console.log("     }");
    console.log("     EOF");
  }

  console.log("");
  console.log("  Launch: cc-code");
  console.log("  Docs:   https://github.com/cc-claws/cc-code");
  console.log("");
}

if (require.main === module) {
  main().catch((err) => {
    console.error("Failed to install cc-code:", err.message);
    process.exit(1);
  });
}

module.exports = { migrateFromClaudeCode };
