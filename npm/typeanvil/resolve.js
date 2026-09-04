'use strict';

// Binary resolution for the `typeanvil` wrapper package.
//
// Order:
// 1. TYPEANVIL_BIN env override (dev/CI).
// 2. require.resolve of the platform package for the current
//    os/arch/libc combination.
// 3. Scan node_modules for any installed @typeanvil/* platform package
//    (covers hoisted/symlinked installs, e.g. Bun's global store).
//
// Throws Error with a human-readable message when no binary is found —
// callers turn that into a clean CLI error, never a stack trace.

const path = require('path');
const fs = require('fs');

// Rust target triple → npm platform package suffix.
const PLATFORM_PACKAGES = {
  'darwin-arm64': '@typeanvil/darwin-arm64',
  'darwin-x64': '@typeanvil/darwin-x64',
  'linux-x64-gnu': '@typeanvil/linux-x64-gnu',
  'linux-x64-musl': '@typeanvil/linux-x64-musl',
  'linux-arm64-gnu': '@typeanvil/linux-arm64-gnu',
};

function currentPlatformKey() {
  const { platform, arch } = process;
  const isMusl = platform === 'linux' && detectMusl();
  const key = `${platform}-${arch}${isMusl ? '-musl' : ''}`;
  return key;
}

// Heuristic musl detection: no glibc report from process.report means musl
// (Alpine etc.). Only consulted on linux; kept cheap and non-fatal.
function detectMusl() {
  try {
    if (process.report && process.report.getReport) {
      const r = process.report.getReport();
      if (r.header && r.header.glibcVersionRuntime) return false;
      return true;
    }
  } catch (_e) {
    /* fall through to default */
  }
  return false; // default to glibc on inconclusive probes
}

function platformPackageName() {
  const key = currentPlatformKey();
  const name = PLATFORM_PACKAGES[key];
  if (!name) {
    throw new Error(
      `typeanvil: unsupported platform "${process.platform}-${process.arch}"` +
        ` (no @typeanvil/* package exists for it).` +
        ` Supported: ${Object.keys(PLATFORM_PACKAGES).join(', ')}.`
    );
  }
  return name;
}

// Walk up from dir looking for node_modules/<pkg>/bin/typeanvil.
function findInNodeModules(dir, pkgName) {
  let current = dir;
  for (let i = 0; i < 32; i++) {
    const candidate = path.join(current, 'node_modules', ...pkgName.split('/'), 'bin', 'typeanvil');
    if (fs.existsSync(candidate)) return candidate;
    const parent = path.dirname(current);
    if (parent === current) break;
    current = parent;
  }
  return null;
}

function resolveBinary() {
  // 1. Explicit override.
  if (process.env.TYPEANVIL_BIN) {
    if (fs.existsSync(process.env.TYPEANVIL_BIN)) return process.env.TYPEANVIL_BIN;
    throw new Error(
      `typeanvil: TYPEANVIL_BIN is set but the file does not exist: ${process.env.TYPEANVIL_BIN}`
    );
  }

  // 2. The platform package for this os/arch/libc.
  let pkgName = null;
  try {
    pkgName = platformPackageName();
  } catch (e) {
    throw e; // unsupported platform — message already precise
  }
  try {
    const pkgRoot = path.dirname(require.resolve(`${pkgName}/package.json`));
    const bin = path.join(pkgRoot, 'bin', 'typeanvil');
    if (fs.existsSync(bin)) return bin;
  } catch (_e) {
    /* platform package not installed — fall through */
  }

  // 3. Scan node_modules from this file's directory upward for ANY
  //    @typeanvil/* package (hoisting/symlinks may install a differently
  //    keyed package than require.resolve expects).
  for (const candidate of Object.values(PLATFORM_PACKAGES)) {
    const bin = findInNodeModules(__dirname, candidate);
    if (bin) return bin;
  }

  // 4. Nothing found.
  throw new Error(
    `typeanvil: native binary not found for platform "${currentPlatformKey()}".\n` +
      `  Tried: ${pkgName}\n` +
      `  This usually means the optional dependency was skipped\n` +
      `  (npm install --omit=optional) or your platform is unsupported.\n` +
      `  Reinstall with: npm install typeanvil\n` +
      `  Supported platforms: ${Object.keys(PLATFORM_PACKAGES).join(', ')}.`
  );
}

module.exports = { resolveBinary, platformPackageName, currentPlatformKey };
