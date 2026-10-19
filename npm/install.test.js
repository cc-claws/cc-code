const { migrateFromClaudeCode, extractRgTarGz } = require("./install");

const fs = require("fs");
const { join } = require("path");
const os = require("os");
const { execSync } = require("child_process");
const { dirname } = require("path");

function makeTempDir() {
  return fs.mkdtempSync(join(os.tmpdir(), "cc-code-install-test-"));
}

function cleanup(dir) {
  fs.rmSync(dir, { recursive: true, force: true });
}

function writeClaudeSettings(home, content) {
  const claudeDir = join(home, ".claude");
  fs.mkdirSync(claudeDir, { recursive: true });
  fs.writeFileSync(join(claudeDir, "settings.json"), JSON.stringify(content, null, 2));
}

function readCcCodeSettings(home) {
  return JSON.parse(fs.readFileSync(join(home, ".cc-code", "settings.json"), "utf-8"));
}

const __tests = [];
function test(name, fn) {
  __tests.push([name, fn]);
}

async function runTests() {
  for (const [name, fn] of __tests) {
    const home = makeTempDir();
    try {
      await fn(home);
      console.log(`  ✓ ${name}`);
    } catch (e) {
      console.error(`  ✗ ${name}`);
      console.error(e.message);
      process.exitCode = 1;
    } finally {
      cleanup(home);
    }
  }
  console.log("");
  if (process.exitCode) {
    console.log("Some tests failed.");
  } else {
    console.log("All tests passed.");
  }
}

console.log("npm/install.js tests");

test("migrateFromClaudeCode returns false when no claude settings exist", (home) => {
  const result = migrateFromClaudeCode(home);
  if (result !== false) throw new Error("expected false");
});

test("migrateFromClaudeCode backfills missing aliases when cc-code settings already exist", (home) => {
  const ccCodeDir = join(home, ".cc-code");
  fs.mkdirSync(ccCodeDir, { recursive: true });
  // 已有配置：anthropic provider，缺少 fable 别名，且 opus 是用户手改过的值
  const existing = {
    config: {
      active_alias: "haiku",
      providers: [
        {
          id: "anthropic",
          type: "anthropic",
          apiKey: "sk-user-own-key",
          models: { opus: "claude-opus-user-custom", sonnet: "claude-sonnet-4-6", haiku: "claude-haiku-4-5" },
        },
      ],
    },
  };
  fs.writeFileSync(join(ccCodeDir, "settings.json"), JSON.stringify(existing, null, 2));
  writeClaudeSettings(home, {
    env: {
      ANTHROPIC_API_KEY: "sk-ant",
      ANTHROPIC_DEFAULT_OPUS_MODEL: "claude-opus-4-7",
      ANTHROPIC_DEFAULT_FABLE_MODEL: "claude-fable-5",
    },
  });
  const result = migrateFromClaudeCode(home);
  if (result !== true) throw new Error("expected true");
  const cfg = readCcCodeSettings(home);
  const p = cfg.config.providers[0];
  // 缺失的 fable 被补上
  if (p.models.fable !== "claude-fable-5") throw new Error("expected fable backfilled");
  // 用户已有的 opus 值不被覆盖
  if (p.models.opus !== "claude-opus-user-custom") throw new Error("must not overwrite existing opus");
  // 用户已有值不被 env 覆盖
  if (p.apiKey !== "sk-user-own-key") throw new Error("must not overwrite user apiKey");
  // active_alias 保持原样
  if (cfg.config.active_alias !== "haiku") throw new Error("must not change active_alias");
});

test("migrateFromClaudeCode backfill leaves unknown provider ids untouched", (home) => {
  const ccCodeDir = join(home, ".cc-code");
  fs.mkdirSync(ccCodeDir, { recursive: true });
  const existing = {
    config: {
      providers: [{ id: "my-custom-id", type: "anthropic", apiKey: "sk-x", models: {} }],
    },
  };
  fs.writeFileSync(join(ccCodeDir, "settings.json"), JSON.stringify(existing));
  writeClaudeSettings(home, { env: { ANTHROPIC_API_KEY: "sk-ant", ANTHROPIC_DEFAULT_FABLE_MODEL: "claude-fable-5" } });
  const result = migrateFromClaudeCode(home);
  if (result !== true) throw new Error("expected true");
  const cfg = readCcCodeSettings(home);
  if (cfg.config.providers.length !== 1) throw new Error("must not add providers");
  if (cfg.config.providers[0].models.fable !== undefined) throw new Error("must not touch unknown provider id");
});

test("migrateFromClaudeCode returns true when cc-code exists but no claude settings", (home) => {
  const ccCodeDir = join(home, ".cc-code");
  fs.mkdirSync(ccCodeDir, { recursive: true });
  fs.writeFileSync(join(ccCodeDir, "settings.json"), JSON.stringify({ config: {} }));
  const result = migrateFromClaudeCode(home);
  if (result !== true) throw new Error("expected true (has cc-code, no quick start)");
});

test("migrateFromClaudeCode produces camelCase provider config for Anthropic", (home) => {
  writeClaudeSettings(home, {
    env: {
      ANTHROPIC_API_KEY: "sk-ant-xxx",
      ANTHROPIC_BASE_URL: "https://api.anthropic.com",
      ANTHROPIC_DEFAULT_OPUS_MODEL: "claude-opus-4-7",
      ANTHROPIC_DEFAULT_SONNET_MODEL: "claude-sonnet-4-6",
      ANTHROPIC_DEFAULT_HAIKU_MODEL: "claude-haiku-4-5",
    },
  });
  const result = migrateFromClaudeCode(home);
  if (result !== true) throw new Error("expected true");
  const cfg = readCcCodeSettings(home);
  if (cfg.config.active_alias !== "opus") throw new Error(`expected active_alias opus, got ${cfg.config.active_alias}`);
  if (cfg.config.active_provider_id !== "anthropic") throw new Error("expected active_provider_id anthropic");
  const p = cfg.config.providers[0];
  if (p.id !== "anthropic") throw new Error("expected provider id anthropic");
  if (p.type !== "anthropic") throw new Error("expected provider type anthropic");
  if (p.apiKey !== "sk-ant-xxx") throw new Error("expected apiKey");
  if (p.baseUrl !== "https://api.anthropic.com") throw new Error("expected baseUrl");
  if (p.provider_type !== undefined) throw new Error("provider_type (snake_case) should not exist");
  if (p.api_key !== undefined) throw new Error("api_key (snake_case) should not exist");
  if (p.base_url !== undefined) throw new Error("base_url (snake_case) should not exist");
  if (p.models.opus !== "claude-opus-4-7") throw new Error("expected models.opus");
  if (p.models.sonnet !== "claude-sonnet-4-6") throw new Error("expected models.sonnet");
  if (p.models.haiku !== "claude-haiku-4-5") throw new Error("expected models.haiku");
});

test("migrateFromClaudeCode produces camelCase provider config for OpenAI", (home) => {
  writeClaudeSettings(home, {
    env: {
      OPENAI_API_KEY: "sk-openai-xxx",
      OPENAI_BASE_URL: "https://api.deepseek.com/v1",
      OPENAI_MODEL: "deepseek-chat",
    },
  });
  const result = migrateFromClaudeCode(home);
  if (result !== true) throw new Error("expected true");
  const cfg = readCcCodeSettings(home);
  if (cfg.config.active_alias !== "sonnet") throw new Error(`expected active_alias sonnet, got ${cfg.config.active_alias}`);
  if (cfg.config.active_provider_id !== "openai") throw new Error("expected active_provider_id openai");
  const p = cfg.config.providers[0];
  if (p.id !== "openai") throw new Error("expected provider id openai");
  if (p.type !== "openai") throw new Error("expected provider type openai");
  if (p.apiKey !== "sk-openai-xxx") throw new Error("expected apiKey");
  if (p.baseUrl !== "https://api.deepseek.com/v1") throw new Error("expected baseUrl");
  if (p.models.sonnet !== "deepseek-chat") throw new Error("expected models.sonnet");
});

test("migrateFromClaudeCode supports CODEX_API_KEY fallback", (home) => {
  writeClaudeSettings(home, {
    env: {
      CODEX_API_KEY: "sk-codex-xxx",
    },
  });
  const result = migrateFromClaudeCode(home);
  if (result !== true) throw new Error("expected true");
  const cfg = readCcCodeSettings(home);
  if (cfg.config.providers[0].apiKey !== "sk-codex-xxx") throw new Error("expected CODEX_API_KEY to be used");
});

// ─────────────────────────────────────────────
// #290：checksum 校验 + 密钥迁移确认制
// ─────────────────────────────────────────────
const { maybeMigrateFromClaudeCode, wouldCopyKeys } = require("./install");

test("wouldCopyKeys is false when cc-code settings already exist", (home) => {
  const ccCodeDir = join(home, ".cc-code");
  fs.mkdirSync(ccCodeDir, { recursive: true });
  fs.writeFileSync(join(ccCodeDir, "settings.json"), JSON.stringify({ config: {} }));
  writeClaudeSettings(home, { env: { ANTHROPIC_API_KEY: "sk-ant" } });
  if (wouldCopyKeys(home) !== false) throw new Error("expected false (backfill only)");
});

test("wouldCopyKeys is true on fresh install with claude keys", (home) => {
  writeClaudeSettings(home, { env: { ANTHROPIC_API_KEY: "sk-ant" } });
  if (wouldCopyKeys(home) !== true) throw new Error("expected true");
});

test("wouldCopyKeys is false without claude settings", (home) => {
  if (wouldCopyKeys(home) !== false) throw new Error("expected false");
});

test("maybeMigrateFromClaudeCode skips key copy in non-TTY without prompt", async (home) => {
  writeClaudeSettings(home, { env: { ANTHROPIC_API_KEY: "sk-ant-must-not-copy" } });
  const result = await maybeMigrateFromClaudeCode(home);
  if (result !== false) throw new Error("expected false (skipped)");
  if (fs.existsSync(join(home, ".cc-code", "settings.json"))) {
    throw new Error("must not create settings.json without consent");
  }
});

test("maybeMigrateFromClaudeCode honors CC_CODE_NO_MIGRATE=1", async (home) => {
  writeClaudeSettings(home, { env: { ANTHROPIC_API_KEY: "sk-ant" } });
  process.env.CC_CODE_NO_MIGRATE = "1";
  try {
    const result = await maybeMigrateFromClaudeCode(home);
    if (result !== false) throw new Error("expected false (skipped by env)");
  } finally {
    delete process.env.CC_CODE_NO_MIGRATE;
  }
});

test("maybeMigrateFromClaudeCode still backfills without consent", async (home) => {
  const ccCodeDir = join(home, ".cc-code");
  fs.mkdirSync(ccCodeDir, { recursive: true });
  fs.writeFileSync(
    join(ccCodeDir, "settings.json"),
    JSON.stringify({ config: { providers: [{ id: "anthropic", type: "anthropic", apiKey: "sk-x", models: {} }] } })
  );
  writeClaudeSettings(home, { env: { ANTHROPIC_API_KEY: "sk-ant", ANTHROPIC_DEFAULT_FABLE_MODEL: "claude-fable-5" } });
  const result = await maybeMigrateFromClaudeCode(home);
  if (result !== true) throw new Error("expected true (backfill needs no consent)");
  const cfg = readCcCodeSettings(home);
  if (cfg.config.providers[0].models.fable !== "claude-fable-5") throw new Error("expected fable backfilled");
});

// ─────────────────────────────────────────────
// Windows 进程文件锁热替换与旧备份清理
// ─────────────────────────────────────────────
const { replaceBinary, cleanOldBinaries } = require("./install");

test("replaceBinary moves extracted file to finalPath when finalPath does not exist", (home) => {
  const binDir = join(home, "bin");
  fs.mkdirSync(binDir, { recursive: true });
  const extracted = join(binDir, "cc-code-new.exe");
  const finalPath = join(binDir, "cc-code.exe");
  fs.writeFileSync(extracted, "new-binary-content");

  replaceBinary(extracted, finalPath);

  if (!fs.existsSync(finalPath)) throw new Error("finalPath must exist");
  if (fs.existsSync(extracted)) throw new Error("extractedPath must have been moved");
  if (fs.readFileSync(finalPath, "utf-8") !== "new-binary-content") throw new Error("content mismatch");
});

test("replaceBinary overwrites finalPath normally when not locked", (home) => {
  const binDir = join(home, "bin");
  fs.mkdirSync(binDir, { recursive: true });
  const extracted = join(binDir, "cc-code-new.exe");
  const finalPath = join(binDir, "cc-code.exe");
  fs.writeFileSync(finalPath, "old-binary-content");
  fs.writeFileSync(extracted, "new-binary-content");

  replaceBinary(extracted, finalPath);

  if (!fs.existsSync(finalPath)) throw new Error("finalPath must exist");
  if (fs.readFileSync(finalPath, "utf-8") !== "new-binary-content") throw new Error("content mismatch");
  // 正常未锁定删除时，不应产生 .old 备份文件
  const files = fs.readdirSync(binDir);
  if (files.some((f) => f.includes(".old."))) throw new Error("should not create .old file when not locked");
});

test("replaceBinary falls back to renaming old file when finalPath is locked (EBUSY/EPERM)", (home) => {
  const binDir = join(home, "bin");
  fs.mkdirSync(binDir, { recursive: true });
  const extracted = join(binDir, "cc-code-new.exe");
  const finalPath = join(binDir, "cc-code.exe");
  fs.writeFileSync(finalPath, "running-binary-content");
  fs.writeFileSync(extracted, "new-binary-content");

  // 模拟 Windows 运行中 exe 无法 unlinkSync（抛 EPERM/EBUSY，包括重命名后的 .old 文件仍然被进程句柄占用）
  const originalUnlink = fs.unlinkSync;
  try {
    fs.unlinkSync = (target) => {
      if (target === finalPath || target.includes("cc-code.exe.old.")) {
        const err = new Error("resource busy or locked");
        err.code = "EBUSY";
        throw err;
      }
      return originalUnlink(target);
    };

    replaceBinary(extracted, finalPath);
  } finally {
    fs.unlinkSync = originalUnlink;
  }

  // 验证：新文件成功放至 finalPath
  if (!fs.existsSync(finalPath)) throw new Error("finalPath must exist");
  if (fs.readFileSync(finalPath, "utf-8") !== "new-binary-content") throw new Error("new content mismatch");

  // 验证：旧文件被成功重命名为 .old.* 备份
  const files = fs.readdirSync(binDir);
  const oldFiles = files.filter((f) => f.startsWith("cc-code.exe.old."));
  if (oldFiles.length !== 1) throw new Error(`expected 1 old backup file, found ${oldFiles.length}`);
  if (fs.readFileSync(join(binDir, oldFiles[0]), "utf-8") !== "running-binary-content") {
    throw new Error("old backup content mismatch");
  }
});

test("cleanOldBinaries unlinks released .old files and ignores still-locked ones", (home) => {
  const binDir = join(home, "bin");
  fs.mkdirSync(binDir, { recursive: true });
  const activeOld = join(binDir, "cc-code.exe.old.1001");
  const inactiveOld = join(binDir, "cc-code.exe.old.1002");
  const normalFile = join(binDir, "cc-code.exe");
  fs.writeFileSync(activeOld, "active");
  fs.writeFileSync(inactiveOld, "inactive");
  fs.writeFileSync(normalFile, "normal");

  const originalUnlink = fs.unlinkSync;
  try {
    fs.unlinkSync = (target) => {
      if (target === activeOld) {
        const err = new Error("file is locked");
        err.code = "EBUSY";
        throw err;
      }
      return originalUnlink(target);
    };

    cleanOldBinaries(binDir);
  } finally {
    fs.unlinkSync = originalUnlink;
  }

  if (!fs.existsSync(normalFile)) throw new Error("normalFile must not be deleted");
  if (!fs.existsSync(activeOld)) throw new Error("activeOld should be kept because it is locked");
  if (fs.existsSync(inactiveOld)) throw new Error("inactiveOld should be unlinked");
});

// --- linkProjectMemories ---
const { linkProjectMemories } = require("./install");

function writeMemoryDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(join(dir, "MEMORY.md"), "# Memory Index\n");
}

test("linkProjectMemories links Claude Code project memory into the cc-code side", (home) => {
  const claudeMem = join(home, ".claude", "projects", "D--code-demo", "memory");
  writeMemoryDir(claudeMem);

  const linked = linkProjectMemories(home);
  if (linked !== 1) throw new Error(`expected 1 link, got ${linked}`);

  const ccMem = join(home, ".cc-code", "projects", "D--code-demo", "memory");
  if (!fs.existsSync(join(ccMem, "MEMORY.md"))) throw new Error("memory not reachable via cc-code path");

  // 写穿透：经 cc-code 侧写入必须落到 Claude Code 侧同一份
  fs.writeFileSync(join(ccMem, "user_role.md"), "shared");
  if (!fs.existsSync(join(claudeMem, "user_role.md"))) {
    throw new Error("cc-code path is a separate physical directory, not a link");
  }
});

test("linkProjectMemories leaves a cc-code-only project untouched", (home) => {
  const ccMem = join(home, ".cc-code", "projects", "D--code-demo", "memory");
  writeMemoryDir(ccMem);

  const linked = linkProjectMemories(home);
  if (linked !== 0) throw new Error(`expected 0 links, got ${linked}`);
  if (fs.existsSync(join(home, ".claude", "projects", "D--code-demo"))) {
    throw new Error("must not add anything under ~/.claude");
  }
});

test("linkProjectMemories never overwrites an existing project memory dir", (home) => {
  writeMemoryDir(join(home, ".claude", "projects", "D--code-demo", "memory"));
  const ccMem = join(home, ".cc-code", "projects", "D--code-demo", "memory");
  writeMemoryDir(ccMem);
  fs.writeFileSync(join(ccMem, "own.md"), "keep me");

  const linked = linkProjectMemories(home);
  if (linked !== 0) throw new Error(`expected 0 links, got ${linked}`);
  if (fs.readFileSync(join(ccMem, "own.md"), "utf-8") !== "keep me") {
    throw new Error("existing cc-code memory content was touched");
  }
  if (fs.lstatSync(ccMem).isSymbolicLink()) throw new Error("existing dir must not be replaced by a link");
});

test("linkProjectMemories is idempotent and no-ops without project memory", (home) => {
  if (linkProjectMemories(home) !== 0) throw new Error("expected 0 links on empty home");

  writeMemoryDir(join(home, ".claude", "projects", "D--code-demo", "memory"));
  if (linkProjectMemories(home) !== 1) throw new Error("first run should link 1");
  if (linkProjectMemories(home) !== 0) throw new Error("second run should link nothing");
});

// 由 runTests() 顺序执行（支持 async 用例）
// --- extractRgTarGz（#620）---------------------------------------------------
// 上游 ripgrep-prebuilt 的 tar.gz 把 rg 放在根目录，旧实现用
// --strip-components=1 导致解不出，bin/rg 永不落地。

function makeTarGz(dir, entries) {
  // entries: [{ name, content, mode }]
  const stage = fs.mkdtempSync(join(os.tmpdir(), "cc-code-rg-stage-"));
  for (const e of entries) {
    const target = join(stage, e.name);
    fs.mkdirSync(dirname(target), { recursive: true });
    fs.writeFileSync(target, e.content);
    if (e.mode) fs.chmodSync(target, e.mode);
  }
  const out = join(dir, "rg-fixture.tar.gz");
  // 用具体条目名打包（不用 "."），避免引入额外的 "./" 层级，
  // 与上游 ripgrep-prebuilt 资产的实际结构保持一致。
  const names = [...new Set(entries.map((e) => e.name.split("/")[0]))].join(" ");
  execSync(`tar -czf "${out}" -C "${stage}" ${names}`, { stdio: "ignore" });
  const buf = fs.readFileSync(out);
  fs.rmSync(out, { force: true });
  fs.rmSync(stage, { recursive: true, force: true });
  return buf;
}

test("extractRgTarGz extracts rg when it sits at archive root", (home) => {
  const dest = join(home, "bin");
  fs.mkdirSync(dest, { recursive: true });
  const buf = makeTarGz(home, [{ name: "rg", content: "#!/bin/sh\necho rg\n", mode: 0o755 }]);
  extractRgTarGz(buf, dest);
  const rg = join(dest, "rg");
  if (!fs.existsSync(rg)) throw new Error("rg was not extracted from root layout");
  if (fs.readFileSync(rg, "utf-8").indexOf("echo rg") === -1) {
    throw new Error("extracted rg content mismatch");
  }
});

test("extractRgTarGz extracts rg when nested in a subdirectory", (home) => {
  const dest = join(home, "bin");
  fs.mkdirSync(dest, { recursive: true });
  const buf = makeTarGz(home, [
    { name: "ripgrep-x.y.z/rg", content: "#!/bin/sh\necho nested\n", mode: 0o755 },
  ]);
  extractRgTarGz(buf, dest);
  const rg = join(dest, "rg");
  if (!fs.existsSync(rg)) throw new Error("rg was not extracted from nested layout");
  if (fs.readFileSync(rg, "utf-8").indexOf("echo nested") === -1) {
    throw new Error("extracted rg content mismatch");
  }
});

test("extractRgTarGz leaves no temp archive behind on failure", (home) => {
  const dest = join(home, "bin");
  fs.mkdirSync(dest, { recursive: true });
  let threw = false;
  try {
    extractRgTarGz(Buffer.from("not a gzip"), dest);
  } catch {
    threw = true;
  }
  if (!threw) throw new Error("corrupt archive should throw");
  const leftover = fs.readdirSync(dest).filter((f) => f.startsWith("rg-download"));
  if (leftover.length) throw new Error(`temp file leaked: ${leftover.join(", ")}`);
});

test("extractRgTarGz leaves no temp archive behind on success", (home) => {
  const dest = join(home, "bin");
  fs.mkdirSync(dest, { recursive: true });
  const buf = makeTarGz(home, [{ name: "rg", content: "#!/bin/sh\n", mode: 0o755 }]);
  extractRgTarGz(buf, dest);
  const leftover = fs.readdirSync(dest).filter((f) => f.startsWith("rg-download"));
  if (leftover.length) throw new Error(`temp file leaked: ${leftover.join(", ")}`);
});

runTests();
