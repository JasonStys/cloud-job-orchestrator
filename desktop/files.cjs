/** File: desktop/files.cjs
 * Purpose: Bound explicit local file reads/exports and reject pre-existing aliases before mutation.
 * Functions: readRegular owns one descriptor; newFile never overwrites; ancestors rejects reparse aliases.
 * State: trusted OS-owner static admission only, not concurrent namespace or cryptographic protection.
 */
"use strict";
const fs = require("node:fs");
const { dirname, resolve } = require("node:path");

function ancestors(path) {
  let current = resolve(path);
  while (true) {
    try {
      const value = fs.lstatSync(current);
      if (value.isSymbolicLink() || !value.isDirectory())
        throw new Error("A real local directory is required.");
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
}

function readRegular(path, maximum) {
  ancestors(dirname(path));
  const named = fs.lstatSync(path);
  if (
    named.isSymbolicLink() ||
    !named.isFile() ||
    named.nlink !== 1 ||
    named.size > maximum
  )
    throw new Error("Linked or oversized input refused.");
  const descriptor = fs.openSync(
    path,
    fs.constants.O_RDONLY |
      (fs.constants.O_NOFOLLOW ?? 0) |
      (fs.constants.O_NONBLOCK ?? 0),
  );
  try {
    const actual = fs.fstatSync(descriptor);
    if (
      !actual.isFile() ||
      actual.nlink !== 1 ||
      actual.dev !== named.dev ||
      actual.ino !== named.ino ||
      actual.size !== named.size
    )
      throw new Error("Input identity changed.");
    const buffer = Buffer.alloc(Math.min(actual.size + 1, maximum + 1));
    let count = 0;
    while (count < buffer.length) {
      const size = fs.readSync(
        descriptor,
        buffer,
        count,
        buffer.length - count,
        null,
      );
      if (!size) break;
      count += size;
    }
    if (count > maximum || count !== actual.size)
      throw new Error("Input byte count differs.");
    return buffer.subarray(0, count);
  } finally {
    fs.closeSync(descriptor);
  }
}

function newFile(path, content) {
  ancestors(dirname(path));
  const descriptor = fs.openSync(path, "wx", 0o600);
  try {
    fs.writeFileSync(descriptor, content);
    fs.fsyncSync(descriptor);
  } finally {
    fs.closeSync(descriptor);
  }
}

/** Reject malformed document bytes instead of silently substituting Unicode replacement characters. */
function readUtf8(path, maximum) {
  return new TextDecoder("utf-8", { fatal: true }).decode(
    readRegular(path, maximum),
  );
}

module.exports = { ancestors, readRegular, readUtf8, newFile };
