#!/usr/bin/env node

const { createWriteStream, mkdirSync, chmodSync, existsSync, renameSync, unlinkSync, writeFileSync } = require("fs");
const { join } = require("path");
const { execSync } = require("child_process");
const crypto = require("crypto");

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

// 校验下载的二进制包（#290）：release 流程会发布 checksums.txt（sha256sum 格式），
// 此处下载并校验；缺失或不匹配时直接失败（fail-closed），拒绝安装可疑二进制。
async function verifyChecksum(buffer, fileName) {
  console.log("  Verifying checksum...");
  let body;
  try {
    body = (await download(`${BASE_URL}/checksums.txt`)).toString("utf-8");
  } catch (e) {
    throw new Error(`无法下载 checksums.txt，拒绝安装（fail-closed）：${e.message}`);
  }
  let expected = null;
  for (const line of body.split("\n")) {
    const m = line.trim().match(/^([0-9a-fA-F]{64})\s+\*?(.+)$/);
    if (m && m[2].trim() === fileName) {
      expected = m[1].toLowerCase();
      break;
    }
  }
  if (!expected) {
    throw new Error(`checksums.txt 中未找到 ${fileName}，拒绝安装（fail-closed）`);
  }
  const actual = crypto.createHash("sha256").update(buffer).digest("hex");
  if (actual !== expected) {
    throw new Error(`checksum 不匹配：${fileName} 可能被篡改，拒绝安装`);
  }
  console.log("  Checksum OK.");
}

// 从 Claude Code env 推导应迁移的 provider 列表（含各档模型别名）
function detectProvidersFromEnv(env) {
  const providers = [];

  // 检测 Anthropic
  const anthropicKey = env.ANTHROPIC_API_KEY || env.ANTHROPIC_AUTH_TOKEN || "";
  const anthropicBaseUrl = env.ANTHROPIC_BASE_URL || "";
  if (anthropicKey || anthropicBaseUrl) {
    const models = {};
    if (env.ANTHROPIC_DEFAULT_OPUS_MODEL) models.opus = env.ANTHROPIC_DEFAULT_OPUS_MODEL;
    if (env.ANTHROPIC_DEFAULT_SONNET_MODEL) models.sonnet = env.ANTHROPIC_DEFAULT_SONNET_MODEL;
    if (env.ANTHROPIC_DEFAULT_HAIKU_MODEL) models.haiku = env.ANTHROPIC_DEFAULT_HAIKU_MODEL;
    if (env.ANTHROPIC_DEFAULT_FABLE_MODEL) models.fable = env.ANTHROPIC_DEFAULT_FABLE_MODEL;
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

  return providers;
}

// 已有 ~/.cc-code/settings.json 时，按 provider id 增量补齐缺失的模型别名。
// 只补空缺（undefined / null / 空串），绝不覆盖用户已有值，也不新增 provider。
// 这样新版本新增的别名（如 fable）能自动补进老配置，而用户的改动（key、baseUrl、
// thinking 等）与已填别名一律保持原样。
function backfillModels(ccCodeSettingsPath, detected) {
  let existing;
  try {
    existing = JSON.parse(require("fs").readFileSync(ccCodeSettingsPath, "utf-8"));
  } catch {
    return true; // 读不动就保持原样，不破坏用户配置
  }

  const providers = existing && existing.config && existing.config.providers;
  if (!Array.isArray(providers)) {
    return true;
  }

  let changed = false;
  for (const det of detected) {
    if (!det.models) continue;
    const target = providers.find((p) => p && p.id === det.id);
    if (!target) continue; // 找不到同 id provider → 不动（不新增）
    if (!target.models || typeof target.models !== "object") {
      target.models = {};
    }
    for (const alias of Object.keys(det.models)) {
      const cur = target.models[alias];
      if (cur === undefined || cur === null || cur === "") {
        target.models[alias] = det.models[alias];
        changed = true;
      }
    }
  }

  if (changed) {
    writeFileSync(ccCodeSettingsPath, JSON.stringify(existing, null, 2) + "\n");
    console.log("");
    console.log("  Backfilled new model aliases into ~/.cc-code/settings.json");
  }
  return true;
}

function migrateFromClaudeCode(home = homedir()) {
  const claudeSettingsPath = join(home, ".claude", "settings.json");
  const ccCodeDir = join(home, ".cc-code");
  const ccCodeSettingsPath = join(ccCodeDir, "settings.json");

  const hasCcCode = existsSync(ccCodeSettingsPath);

  // 无 Claude Code 配置：已有 cc-code 视为成功（无需 Quick Start），否则失败
  if (!existsSync(claudeSettingsPath)) {
    return hasCcCode;
  }

  let claudeSettings;
  try {
    claudeSettings = JSON.parse(require("fs").readFileSync(claudeSettingsPath, "utf-8"));
  } catch {
    return hasCcCode;
  }

  const env = claudeSettings.env || {};
  const providers = detectProvidersFromEnv(env);

  if (providers.length === 0) {
    return hasCcCode;
  }

  // 已有 cc-code 配置 → 增量补齐缺失别名（不覆盖、不新增 provider）
  if (hasCcCode) {
    return backfillModels(ccCodeSettingsPath, providers);
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
  console.log("  Detected Claude Code configuration (compatible with cc-code).");
  console.log("  Migrated ~/.claude/settings.json -> ~/.cc-code/settings.json");
  console.log(`  Found ${providers.length} provider(s): ${providers.map(p => p.type).join(", ")}`);
  console.log("  NOTE: API keys were copied into ~/.cc-code/settings.json (plain text).");
  console.log("        Review it if you do not want credentials duplicated there.");
  return true;
}

// 仅当迁移会创建新 settings.json 并写入 API 密钥时返回 true。
// （已有 ~/.cc-code/settings.json 时只做别名 backfill，不碰密钥，无需确认。）
function wouldCopyKeys(home = homedir()) {
  if (existsSync(join(home, ".cc-code", "settings.json"))) return false;
  const claudeSettingsPath = join(home, ".claude", "settings.json");
  if (!existsSync(claudeSettingsPath)) return false;
  try {
    const claudeSettings = JSON.parse(require("fs").readFileSync(claudeSettingsPath, "utf-8"));
    return detectProvidersFromEnv(claudeSettings.env || {}).length > 0;
  } catch {
    return false;
  }
}

function promptYesNo(question) {
  const readline = require("readline");
  const rl = readline.createInterface({ input: process.stdin, output: process.stdout });
  return new Promise((resolve) => {
    rl.question(question, (answer) => {
      rl.close();
      resolve(/^(y|yes)$/i.test(answer.trim()));
    });
  });
}

// 密钥迁移需要显式确认（#290）：postinstall 不再静默复制 API 密钥。
// - CC_CODE_NO_MIGRATE=1 或 --no-migrate：跳过
// - 非 TTY（如 CI/脚本）：跳过并提示手动迁移（安全默认值）
// - TTY：询问用户，默认 No
async function maybeMigrateFromClaudeCode(home = homedir()) {
  if (!wouldCopyKeys(home)) {
    // 无需复制密钥（仅 backfill 别名或无 Claude 配置）：保持原行为
    return migrateFromClaudeCode(home);
  }
  if (process.env.CC_CODE_NO_MIGRATE === "1" || process.argv.includes("--no-migrate")) {
    console.log("  已跳过 Claude Code 密钥迁移 (CC_CODE_NO_MIGRATE=1 / --no-migrate)。");
    return false;
  }
  if (!process.stdin.isTTY) {
    console.log("");
    console.log("  检测到 ~/.claude/settings.json，但当前为非交互环境，不会自动复制 API 密钥。");
    console.log("  如需迁移，请在终端中重新运行安装，或手动复制配置到 ~/.cc-code/settings.json。");
    return false;
  }
  console.log("");
  const ok = await promptYesNo("  检测到 Claude Code 配置，是否将 API 密钥迁移到 ~/.cc-code/settings.json？[y/N] ");
  if (!ok) {
    console.log("  已跳过密钥迁移。");
    return false;
  }
  return migrateFromClaudeCode(home);
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

// #410：把 npm 自动生成的全局入口（<prefix>/cc-code.cmd|.ps1）改写为直起 exe。
// 全局安装时 __dirname = <prefix>/node_modules/@cc-claw/code，故 prefix 根为
// __dirname/../../..。仅在能定位到 npm 生成的 wrapper 时才覆盖；任何失败都不
// 阻塞安装（此时 exe 同目录 bin/cc-code.cmd|.ps1 仍可直起）。
function overwriteNpmGlobalWrapper(binDirPath) {
  try {
    const prefixRoot = join(__dirname, "..", "..", "..");
    const exePath = join(binDirPath, "cc-code.exe");
    // 全局 cmd 用相对自身的路径引用 exe，避免硬编码绝对路径（prefix 可能含空格）
    const cmdTarget = join(prefixRoot, "cc-code.cmd");
    const ps1Target = join(prefixRoot, "cc-code.ps1");

    // 防御：仅当 exe 存在且全局 wrapper 也存在（确是 npm 全局安装布局）时才覆盖
    if (!existsSync(exePath)) return;
    if (!existsSync(cmdTarget) && !existsSync(ps1Target)) return;

    // exe 相对 prefix 根的路径：node_modules/@cc-claw/code/bin/cc-code.exe
    const relExe = join("node_modules", "@cc-claw", "code", "bin", "cc-code.exe");

    writeFileSync(
      cmdTarget,
      `@echo off\r\n"%~dp0${relExe.replace(/\//g, "\\")}" %*\r\n`
    );
    writeFileSync(
      ps1Target,
      `$basedir = Split-Path $MyInvocation.MyCommand.Definition -Parent\r\n& "$basedir\\${relExe.replace(/\//g, "\\")}" @args\r\nexit $LASTEXITCODE\r\n`
    );
    console.log("  Rewrote npm global wrapper to launch cc-code.exe directly (Ctrl+C fix).");
  } catch (err) {
    console.log(`  global wrapper rewrite skipped: ${err.message}`);
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

  // #290：校验二进制完整性（fail-closed：缺失/不匹配则拒绝安装）
  await verifyChecksum(buffer, fileName);

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

    // #410：npm 全局入口（<prefix>/cc-code.cmd|.ps1）由 npm 依据 package.json
    // 的 bin（指向 node 脚本 bin/cc-code）自动生成，会经 node 以 execFileSync
    // 拉起 exe。Ctrl+C 的 CTRL_C_EVENT 广播给整个控制台进程组时，node 父进程
    // 无 handler 被终止，连带拖死 cc-code.exe（其 SetConsoleCtrlHandler 拦截
    // 因此失效），表现为任意状态按 Ctrl+C 直接退出 TUI。这里把全局入口改写为
    // 直起 exe，去掉 node 中间层，进程组内只剩 cc-code.exe，handler 正常生效。
    overwriteNpmGlobalWrapper(binDirPath);
  }

  console.log(`cc-code ${VERSION} installed successfully.`);

  // 下载 ripgrep 预编译二进制（增强 Grep/Glob 性能，失败不阻塞安装）
  await downloadRipgrep(platform, binDir);

  // #290：密钥迁移需显式确认，不再静默复制
  const migrated = await maybeMigrateFromClaudeCode();

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

module.exports = { migrateFromClaudeCode, maybeMigrateFromClaudeCode, wouldCopyKeys, verifyChecksum };
