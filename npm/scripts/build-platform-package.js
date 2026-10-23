#!/usr/bin/env node

// Assemble one npm platform package from a release artifact.
//
// Each platform package contains the cc-code binary plus a prebuilt ripgrep,
// so a plain `npm install -g @cc-claw/code` needs no install script, no second
// network request and no system `tar`. npm's own integrity check covers the
// whole tarball, which replaces the old in-installer checksums.txt download.
//
// Usage:
//   node npm/scripts/build-platform-package.js \
//     --platform win32-x64 \
//     --binary  dist/cc-code-windows-x86_64.exe \
//     --version 0.6.178 \
//     --out     platform-packages
//
// Optional:
//   --rg <path>          prebuilt ripgrep binary to bundle
//   --rg-version <ver>   recorded in the package README
//
// `--rg-version` is passed explicitly rather than probed by running the binary:
// the release job assembles every platform on one runner, and a darwin or
// Windows rg cannot be executed on Linux.

const fs = require("fs");
const path = require("path");

/// Platform package metadata. `os` / `cpu` drive npm's automatic platform
/// selection: on a given host npm installs exactly the packages whose fields
/// match and silently skips the rest.
///
/// `libc` is deliberately NOT set. The released Linux binaries link against
/// glibc, so a `libc: ["glibc"]` field would be semantically right — but npm
/// rejects a package whose `libc` cannot be detected on the host
/// (npm-install-checks: `target.libc && !libc` → EBADPLATFORM), and detection
/// relies on `/usr/bin/ldd` or `process.report`. Failing open here keeps the
/// package installable; the launcher reports a musl/Alpine host explicitly when
/// the glibc binary cannot be executed.
const PLATFORMS = {
  "win32-x64": { os: "win32", cpu: "x64", binary: "cc-code.exe", rg: "rg.exe" },
  "win32-arm64": { os: "win32", cpu: "arm64", binary: "cc-code.exe", rg: "rg.exe" },
  "darwin-x64": { os: "darwin", cpu: "x64", binary: "bin/cc-code", rg: "bin/rg" },
  "darwin-arm64": { os: "darwin", cpu: "arm64", binary: "bin/cc-code", rg: "bin/rg" },
  "linux-x64": { os: "linux", cpu: "x64", binary: "bin/cc-code", rg: "bin/rg" },
  "linux-arm64": { os: "linux", cpu: "arm64", binary: "bin/cc-code", rg: "bin/rg" },
};
function parseArgs(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i += 1) {
    const flag = argv[i];
    if (!flag.startsWith("--")) continue;
    const value = argv[i + 1];
    if (value === undefined || value.startsWith("--")) {
      throw new Error(`Missing value for ${flag}`);
    }
    args[flag.slice(2)] = value;
    i += 1;
  }
  return args;
}

function packageName(platform) {
  return `@cc-claw/code-${platform}`;
}

function buildReadme(platform, version, rgVersion) {
  const rgLine = rgVersion
    ? `\nIt also bundles ripgrep ${rgVersion} for the Grep / Glob tools, so no system \`rg\` is needed.\n`
    : "";
  return `# ${packageName(platform)}

Platform binary for [\`@cc-claw/code\`](https://www.npmjs.com/package/@cc-claw/code) ${version} (${platform}).

This package is installed automatically as an optional dependency of
\`@cc-claw/code\` on matching platforms. Install the main package instead:

\`\`\`bash
npm install -g @cc-claw/code
\`\`\`
${rgLine}
Installing this package directly does nothing on its own — it exists only to
carry the binary.
`;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const platform = args.platform;
  const version = args.version;

  if (!platform || !PLATFORMS[platform]) {
    throw new Error(
      `--platform must be one of: ${Object.keys(PLATFORMS).join(", ")}`,
    );
  }
  if (!version) throw new Error("--version is required");
  if (!args.binary) throw new Error("--binary is required");
  if (!args.out) throw new Error("--out is required");

  if (args.rg && !fs.existsSync(args.rg)) {
    throw new Error(`--rg not found: ${args.rg}`);
  }
  if (args.binary && !fs.existsSync(args.binary)) {
    throw new Error(`--binary not found: ${args.binary}`);
  }

  const meta = PLATFORMS[platform];
  const outDir = path.join(args.out, platform);
  fs.rmSync(outDir, { recursive: true, force: true });

  // 1. Binary
  const binaryDest = path.join(outDir, meta.binary);
  fs.mkdirSync(path.dirname(binaryDest), { recursive: true });
  fs.copyFileSync(args.binary, binaryDest);
  if (meta.os !== "win32") fs.chmodSync(binaryDest, 0o755);

  // 2. ripgrep (optional: a missing rg degrades Grep to the built-in engine,
  //    it must never fail the build)
  if (args.rg) {
    const rgDest = path.join(outDir, meta.rg);
    fs.mkdirSync(path.dirname(rgDest), { recursive: true });
    fs.copyFileSync(args.rg, rgDest);
    if (meta.os !== "win32") fs.chmodSync(rgDest, 0o755);
  }
  const rgVersion = args["rg-version"] || null;

  // 3. package.json
  const manifest = {
    name: packageName(platform),
    version,
    description: `cc-code platform binary for ${platform}`,
    repository: {
      type: "git",
      url: "git+https://github.com/cc-claws/cc-code.git",
    },
    homepage: "https://github.com/cc-claws/cc-code",
    license: "Apache-2.0",
    os: [meta.os],
    cpu: [meta.cpu],
  };
  if (meta.libc) manifest.libc = meta.libc;
  // `files` keeps the built tarball to the payload plus the generated README.
  manifest.files = [meta.binary];
  if (args.rg) manifest.files.push(meta.rg);
  fs.writeFileSync(
    path.join(outDir, "package.json"),
    `${JSON.stringify(manifest, null, 2)}\n`,
  );

  fs.writeFileSync(
    path.join(outDir, "README.md"),
    buildReadme(platform, version, rgVersion),
  );

  console.log(`Built ${packageName(platform)}@${version} in ${outDir}`);
  for (const f of manifest.files) {
    const size = fs.statSync(path.join(outDir, f)).size;
    console.log(`  ${f.replace(/\\/g, "/")} (${(size / 1024 / 1024).toFixed(1)} MiB)`);
  }
}

if (require.main === module) {
  try {
    main();
  } catch (err) {
    console.error(`build-platform-package: ${err.message}`);
    process.exit(1);
  }
}

module.exports = { PLATFORMS, packageName, parseArgs };
