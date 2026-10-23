// Tests for the npm launcher and the platform-package builder.
//
//   node npm/launcher.test.js
//
// These tests build throwaway install trees in a temp directory and drive the
// real launcher as a child process, so they cover what users actually hit
// (resolution, error text, exit codes) rather than internal helpers alone.

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync, spawnSync } = require("child_process");

const LAUNCHER = path.join(__dirname, "bin", "cc-code");
const BUILDER = path.join(__dirname, "scripts", "build-platform-package.js");

const tests = [];
function test(name, fn) {
  tests.push([name, fn]);
}

function tmpdir(prefix) {
  return fs.mkdtempSync(path.join(os.tmpdir(), prefix));
}

function platformBinRel() {
  return process.platform === "win32" ? "cc-code.exe" : "bin/cc-code";
}

/// Lay out a fake installed tree:
///
///   <root>/node_modules/@cc-claw/code/bin/cc-code       copy of the launcher
///   <root>/node_modules/@cc-claw/code-<platform>/...    optional platform pkg
///
/// `installPayload` receives the platform package dir and may write a binary
/// there. Returns { root, launcher, pkgDir }.
function fakeInstall(platform, installPayload) {
  const root = tmpdir("cc-code-launcher-");
  const mainDir = path.join(root, "node_modules", "@cc-claw", "code", "bin");
  fs.mkdirSync(mainDir, { recursive: true });
  const launcher = path.join(mainDir, "cc-code");
  fs.copyFileSync(LAUNCHER, launcher);
  fs.chmodSync(launcher, 0o755);

  let pkgDir = null;
  if (platform) {
    pkgDir = path.join(root, "node_modules", "@cc-claw", `code-${platform}`);
    fs.mkdirSync(pkgDir, { recursive: true });
    fs.writeFileSync(
      path.join(pkgDir, "package.json"),
      JSON.stringify({ name: `@cc-claw/code-${platform}`, version: "0.0.0" }),
    );
    if (installPayload) installPayload(pkgDir);
  }
  return { root, launcher, pkgDir };
}

/// Place a *real* native executable at the platform package's binary path.
///
/// The launcher spawns the binary directly, so a shebang script is not enough
/// (Windows rejects it with EFTYPE). Node is used as the executable: the
/// launcher's own argv is then handed to Node, which runs the first argument as
/// a script — so a test can pass a script path and observe both argument
/// forwarding and exit-code mirroring without shipping a real build.
function writeNodeExecutable(pkgDir) {
  const dest = path.join(pkgDir, platformBinRel());
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  try {
    fs.linkSync(process.execPath, dest);
  } catch {
    fs.copyFileSync(process.execPath, dest);
  }
  fs.chmodSync(dest, 0o755);
  return dest;
}

function runLauncher(launcher, args, env = {}) {
  return spawnSync(process.execPath, [launcher, ...args], {
    encoding: "utf-8",
    env: { ...process.env, ...env },
  });
}

/// Write a script that optionally echoes its argv and exits with `code`.
function writeStubScript(dir, { echoArgs = false, code = 0 } = {}) {
  const file = path.join(dir, `stub-${Math.random().toString(36).slice(2)}.js`);
  fs.writeFileSync(
    file,
    echoArgs
      ? `process.stdout.write(JSON.stringify(process.argv.slice(2)));\nprocess.exit(${code});\n`
      : `process.exit(${code});\n`,
  );
  return file;
}

// ─── launcher: happy path ───────────────────────────────────────────────────

test("launcher forwards args and mirrors the child exit code", () => {
  const plat = `${process.platform}-${process.arch}`;
  const { launcher, root } = fakeInstall(plat, writeNodeExecutable);
  const stub = writeStubScript(root, { echoArgs: true, code: 7 });

  const r = runLauncher(launcher, [stub, "a", "b c"]);
  assert.strictEqual(r.status, 7, `expected exit 7, got ${r.status}: ${r.stderr}`);
  assert.deepStrictEqual(JSON.parse(r.stdout), ["a", "b c"]);
});

test("launcher passes a zero exit code through as zero", () => {
  const plat = `${process.platform}-${process.arch}`;
  const { launcher, root } = fakeInstall(plat, writeNodeExecutable);
  const stub = writeStubScript(root, { code: 0 });
  assert.strictEqual(runLauncher(launcher, [stub]).status, 0);
});

// ─── launcher: missing binary ───────────────────────────────────────────────

test("launcher reports the missing optional dependency instead of a bare ENOENT", () => {
  const plat = `${process.platform}-${process.arch}`;
  // Platform package present, payload absent: the shape produced by
  // `--omit=optional` dropping the payload or a partially published package.
  const { launcher } = fakeInstall(plat, null);

  const r = runLauncher(launcher, []);
  assert.strictEqual(r.status, 1);
  assert.match(r.stderr, /no binary for/);
  assert.match(r.stderr, /optional dependency/);
  assert.match(r.stderr, /--no-optional/);
});

test("unsupported-platform message stays specific for known-missing targets", () => {
  const { unsupportedPlatformMessage } = require("./bin/cc-code");
  // win32-arm64 is a recognized platform with no published package; it must
  // not fall through to the generic "unsupported platform" text.
  const msg = unsupportedPlatformMessage("win32-arm64");
  assert.match(msg, /--arch=x64/);
  assert.match(msg, /no binary for win32-arm64/);

  const generic = unsupportedPlatformMessage("freebsd-x64");
  assert.match(generic, /unsupported platform freebsd-x64/);
  assert.match(generic, /win32-arm64/, "known-missing targets stay listed");
});

test("missing-binary message points arm64 Linux users at the script install", () => {
  const { missingBinaryMessage, TARGETS } = require("./bin/cc-code");
  const msg = missingBinaryMessage("linux-arm64", TARGETS["linux-arm64"]);
  assert.match(msg, /https:\/\/github\.com\/cc-claws\/cc-code#install/);
});

// ─── launcher: resolution ──────────────────────────────────────────────────

test("launcher resolves the binary through the platform package root", () => {
  // Guards the require.resolve(<pkg>/package.json) approach: a regression to a
  // hardcoded node_modules path would silently break pnpm and bun layouts.
  const plat = `${process.platform}-${process.arch}`;
  const { root } = fakeInstall(plat, writeNodeExecutable);
  const expected = path.join(
    root, "node_modules", "@cc-claw", `code-${plat}`, platformBinRel(),
  );

  // The probe must sit inside the main package and require the *copied*
  // launcher, so Node resolves siblings exactly the way the real one does.
  const probe = path.join(
    root, "node_modules", "@cc-claw", "code", "bin", "probe.js",
  );
  const copiedLauncher = path.join(
    root, "node_modules", "@cc-claw", "code", "bin", "cc-code",
  );
  fs.writeFileSync(
    probe,
    `process.stdout.write(require(${JSON.stringify(copiedLauncher)})` +
      `.resolveInPackage(${JSON.stringify(`@cc-claw/code-${plat}`)}, ` +
      `${JSON.stringify(platformBinRel())}) || "")`,
  );
  const resolved = execFileSync(process.execPath, [probe], { encoding: "utf-8" }).trim();
  assert.strictEqual(resolved, expected);
});

test("resolveInPackage returns null for an absent platform package", () => {
  const { resolveInPackage } = require("./bin/cc-code");
  assert.strictEqual(resolveInPackage("@cc-claw/code-nonexistent-xyz", "bin/cc-code"), null);
});

// ─── launcher: escape hatch ────────────────────────────────────────────────

test("CC_CODE_BINARY overrides platform resolution", () => {
  const { launcher, root } = fakeInstall(null);
  const stub = writeStubScript(root, { echoArgs: true });
  const r = runLauncher(launcher, [stub, "x"], { CC_CODE_BINARY: process.execPath });
  assert.strictEqual(r.status, 0, r.stderr);
  assert.deepStrictEqual(JSON.parse(r.stdout), ["x"]);
});

// ─── launcher: module surface ──────────────────────────────────────────────

test("TARGETS lists exactly the published platform packages", () => {
  const { TARGETS } = require("./bin/cc-code");
  assert.deepStrictEqual(Object.keys(TARGETS).sort(), [
    "darwin-arm64",
    "darwin-x64",
    "linux-arm64",
    "linux-x64",
    "win32-x64",
  ]);
  for (const [key, t] of Object.entries(TARGETS)) {
    assert.strictEqual(t.pkg, `@cc-claw/code-${key}`, `${key} package name`);
  }
});

test("TARGETS file paths match what the builder produces", () => {
  const { TARGETS } = require("./bin/cc-code");
  const { PLATFORMS } = require("./scripts/build-platform-package.js");
  for (const [key, t] of Object.entries(TARGETS)) {
    assert.ok(PLATFORMS[key], `${key} must exist in the builder`);
    assert.strictEqual(
      t.file,
      PLATFORMS[key].binary,
      `${key}: launcher and builder must agree on the binary path`,
    );
  }
});

// ─── platform package builder ──────────────────────────────────────────────

test("builder writes a manifest npm can select on the target platform", () => {
  const work = tmpdir("cc-code-build-");
  const binary = path.join(work, "cc-code-windows-x86_64.exe");
  fs.writeFileSync(binary, "fake binary");
  const out = path.join(work, "out");

  execFileSync(process.execPath, [
    BUILDER,
    "--platform", "win32-x64",
    "--binary", binary,
    "--version", "1.2.3",
    "--out", out,
  ]);

  const pkgDir = path.join(out, "win32-x64");
  const manifest = JSON.parse(fs.readFileSync(path.join(pkgDir, "package.json"), "utf-8"));
  assert.strictEqual(manifest.name, "@cc-claw/code-win32-x64");
  assert.strictEqual(manifest.version, "1.2.3");
  assert.deepStrictEqual(manifest.os, ["win32"]);
  assert.deepStrictEqual(manifest.cpu, ["x64"]);
  // libc must stay unset: npm drops a package whose libc it cannot resolve,
  // which would turn a missing probe into a failed install.
  assert.strictEqual(manifest.libc, undefined);
  assert.ok(fs.existsSync(path.join(pkgDir, "cc-code.exe")));
});

test("builder places the Unix binary under bin/ and makes it executable", () => {
  const work = tmpdir("cc-code-build-unix-");
  const binary = path.join(work, "cc-code-linux-x86_64");
  fs.writeFileSync(binary, "fake binary");
  const out = path.join(work, "out");

  execFileSync(process.execPath, [
    BUILDER,
    "--platform", "linux-arm64",
    "--binary", binary,
    "--version", "1.2.3",
    "--out", out,
  ]);

  const dest = path.join(out, "linux-arm64", "bin", "cc-code");
  assert.ok(fs.existsSync(dest));
  if (process.platform !== "win32") {
    assert.ok(fs.statSync(dest).mode & 0o111, "binary must be executable");
  }
});

test("builder bundles ripgrep and records its version", () => {
  const work = tmpdir("cc-code-build-rg-");
  const binary = path.join(work, "cc-code-darwin-aarch64");
  fs.writeFileSync(binary, "fake binary");
  const rg = path.join(work, "rg");
  fs.writeFileSync(rg, "fake rg");
  const out = path.join(work, "out");

  execFileSync(process.execPath, [
    BUILDER,
    "--platform", "darwin-arm64",
    "--binary", binary,
    "--rg", rg,
    "--rg-version", "15.0.1",
    "--version", "1.2.3",
    "--out", out,
  ]);

  const pkgDir = path.join(out, "darwin-arm64");
  const manifest = JSON.parse(fs.readFileSync(path.join(pkgDir, "package.json"), "utf-8"));
  assert.deepStrictEqual(manifest.files, ["bin/cc-code", "bin/rg"]);
  assert.ok(fs.existsSync(path.join(pkgDir, "bin", "rg")));
  assert.match(
    fs.readFileSync(path.join(pkgDir, "README.md"), "utf-8"),
    /ripgrep 15\.0\.1/,
  );
});

test("builder works without ripgrep (bundling is optional)", () => {
  const work = tmpdir("cc-code-build-norg-");
  const binary = path.join(work, "cc-code-linux-x86_64");
  fs.writeFileSync(binary, "fake binary");
  const out = path.join(work, "out");

  execFileSync(process.execPath, [
    BUILDER,
    "--platform", "linux-x64",
    "--binary", binary,
    "--version", "1.2.3",
    "--out", out,
  ]);

  const manifest = JSON.parse(
    fs.readFileSync(path.join(out, "linux-x64", "package.json"), "utf-8"),
  );
  assert.deepStrictEqual(manifest.files, ["bin/cc-code"]);
  assert.ok(!fs.existsSync(path.join(out, "linux-x64", "bin", "rg")));
});

test("builder rejects an unknown platform", () => {
  const work = tmpdir("cc-code-build-bad-");
  const r = spawnSync(process.execPath, [
    BUILDER,
    "--platform", "linux-riscv64",
    "--binary", "x",
    "--version", "1.0.0",
    "--out", work,
  ], { encoding: "utf-8" });
  assert.strictEqual(r.status, 1);
  assert.match(r.stderr, /--platform must be one of/);
});

test("builder fails loudly on a missing binary instead of shipping an empty package", () => {
  const work = tmpdir("cc-code-build-missing-");
  const r = spawnSync(process.execPath, [
    BUILDER,
    "--platform", "linux-x64",
    "--binary", path.join(work, "nope"),
    "--version", "1.0.0",
    "--out", work,
  ], { encoding: "utf-8" });
  assert.strictEqual(r.status, 1);
  assert.match(r.stderr, /--binary not found/);
});

// ─── main package manifest ─────────────────────────────────────────────────

test("main package runs no install script", () => {
  const manifest = JSON.parse(
    fs.readFileSync(path.join(__dirname, "package.json"), "utf-8"),
  );
  assert.strictEqual(manifest.scripts && manifest.scripts.postinstall, undefined);
  assert.strictEqual(manifest.scripts && manifest.scripts.install, undefined);
});

test("main package pins every platform package to its own version", () => {
  const manifest = JSON.parse(
    fs.readFileSync(path.join(__dirname, "package.json"), "utf-8"),
  );
  const { TARGETS } = require("./bin/cc-code");
  const declared = manifest.optionalDependencies || {};
  for (const t of Object.values(TARGETS)) {
    assert.strictEqual(
      declared[t.pkg],
      manifest.version,
      `${t.pkg} must be pinned to ${manifest.version}`,
    );
  }
});

test("main package ships only the launcher and its README", () => {
  const manifest = JSON.parse(
    fs.readFileSync(path.join(__dirname, "package.json"), "utf-8"),
  );
  assert.ok(manifest.files.includes("bin/cc-code"));
  assert.ok(manifest.files.includes("README.md"));
  assert.ok(!manifest.files.includes("scripts"), "build scripts must not ship");
  assert.ok(!manifest.files.some((f) => f.includes("test")), "tests must not ship");
});

// ─── runner ───────────────────────────────────────────────────────────────

console.log("npm launcher tests\n");
let failed = 0;
for (const [name, fn] of tests) {
  try {
    fn();
    console.log(`  \u2713 ${name}`);
  } catch (err) {
    failed += 1;
    console.error(`  \u2717 ${name}`);
    console.error(`    ${String(err.message).split("\n").join("\n    ")}`);
  }
}
console.log("");
if (failed > 0) {
  console.error(`${failed} test(s) failed.`);
  process.exit(1);
}
console.log(`${tests.length} tests passed.`);
