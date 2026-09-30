const { migrateFromClaudeCode } = require("./install");

const fs = require("fs");
const { join } = require("path");
const os = require("os");

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

// 由 runTests() 顺序执行（支持 async 用例）
runTests();
