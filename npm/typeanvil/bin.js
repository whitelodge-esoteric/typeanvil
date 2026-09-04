#!/usr/bin/env node
'use strict';

// CLI passthrough: exec the native binary with all arguments inherited.
// Exit code, stdout, and stderr pass straight through (spec Behavior 1).
// SIGINT/SIGTERM are forwarded to the child so Ctrl-C kills the render.

const { spawn } = require('child_process');
const { resolveBinary } = require('./resolve');

let binPath;
try {
  binPath = resolveBinary();
} catch (err) {
  console.error(err.message);
  process.exit(1);
}

const child = spawn(binPath, process.argv.slice(2), {
  stdio: 'inherit',
});

child.on('error', (err) => {
  console.error(`typeanvil: failed to exec binary: ${err.message}`);
  process.exit(1);
});

const forward = (signal) => {
  if (!child.killed) child.kill(signal);
};
process.on('SIGINT', () => forward('SIGINT'));
process.on('SIGTERM', () => forward('SIGTERM'));

child.on('close', (code, signal) => {
  if (signal) {
    // Mirror the shell convention: 128 + signal number.
    process.exit(128 + (require('os').constants.signals[signal] || 0));
  }
  process.exit(code === null ? 1 : code);
});
