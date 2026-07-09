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

function test(name, fn) {
  const home = makeTempDir();
  try {
    fn(home);
    console.log(`  ✓ ${name}`);
  } catch (e) {
    console.error(`  ✗ ${name}`);
    console.error(e.message);
    process.exitCode = 1;
  } finally {
    cleanup(home);
  }
}

console.log("npm/install.js tests");

test("migrateFromClaudeCode returns false when no claude settings exist", (home) => {
  const result = migrateFromClaudeCode(home);
  if (result !== false) throw new Error("expected false");
});

test("migrateFromClaudeCode skips when cc-code settings already exist", (home) => {
  const ccCodeDir = join(home, ".cc-code");
  fs.mkdirSync(ccCodeDir, { recursive: true });
  fs.writeFileSync(join(ccCodeDir, "settings.json"), JSON.stringify({ config: {} }));
  writeClaudeSettings(home, { env: { ANTHROPIC_API_KEY: "sk-ant" } });
  const result = migrateFromClaudeCode(home);
  if (result !== true) throw new Error("expected true");
  const content = fs.readFileSync(join(ccCodeDir, "settings.json"), "utf-8");
  if (content !== JSON.stringify({ config: {} })) {
    throw new Error("should not overwrite existing settings");
  }
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

console.log("");
if (process.exitCode) {
  console.log("Some tests failed.");
} else {
  console.log("All tests passed.");
}
