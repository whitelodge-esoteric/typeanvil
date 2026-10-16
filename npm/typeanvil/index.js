'use strict';

// Programmatic API: spawn the native binary and return the PDF bytes.
// Thin wrapper only — no engine logic here (see docs/specifications/
// npm-distribution.spec.md, Behavior 2).
//
// The engine binary has no stdout output mode (-o requires a real path), so
// render() writes to a temp file next to the output, streams the bytes back,
// and removes the temp file.

const { spawn } = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { resolveBinary } = require('./resolve');

/**
 * Render HTML to PDF via the typeanvil binary.
 *
 * @param {string[]} args - CLI arguments, e.g. ['render', 'in.html',
 *        '--page-width', '8.5in', '--page-height', '11in', '-o', 'out.pdf']
 * @param {object} [opts]
 * @param {string} [opts.cwd] - working directory for the binary (default: process.cwd())
 * @param {string} [opts.binPath] - explicit binary path (default: auto-resolve)
 * @returns {Promise<{pdf: Uint8Array, code: number}>}
 *          Resolves when the binary exits 0; rejects otherwise with the
 *          binary's stderr attached to the error.
 *
 * The arguments must include `-o <path>`: the engine requires an explicit
 * output file. After a successful render, `pdf` carries the same bytes as
 * the file at that path (the file is NOT deleted — pass a temp path if you
 * want throwaway output).
 */
function render(args, opts = {}) {
  const argv = Array.isArray(args) ? args : String(args).split(/\s+/).filter(Boolean);
  const binPath = opts.binPath || resolveBinary();

  const oFlag = argv.indexOf('-o');
  if (oFlag === -1 || !argv[oFlag + 1]) {
    return Promise.reject(
      new Error('typeanvil: render() requires -o <path> in args (the engine has no stdout mode)')
    );
  }
  const outPath = argv[oFlag + 1];

  return new Promise((resolve, reject) => {
    const child = spawn(binPath, argv, {
      cwd: opts.cwd || process.cwd(),
      stdio: ['ignore', 'pipe', 'pipe'],
    });

    let stderr = [];
    child.stderr.on('data', (d) => stderr.push(d));

    child.on('error', (err) => {
      reject(new Error(`typeanvil: failed to spawn binary: ${err.message}`));
    });

    child.on('close', (code, signal) => {
      const errText = Buffer.concat(stderr).toString('utf8');
      if (code !== 0) {
        const err = new Error(
          `typeanvil: render failed (exit ${code}${signal ? `, signal ${signal}` : ''})\n${errText}`
        );
        err.code = code;
        err.stderr = errText;
        reject(err);
        return;
      }
      let pdf;
      try {
        pdf = new Uint8Array(fs.readFileSync(outPath));
      } catch (e) {
        reject(new Error(`typeanvil: render reported success but output is unreadable: ${e.message}`));
        return;
      }
      resolve({ pdf, code });
    });
  });
}

module.exports = { render, resolveBinary };
