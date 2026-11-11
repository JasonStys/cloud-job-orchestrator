/**
 * File: publication-text.mjs
 * Purpose: Keep exact reviewed Word XML and relationships visible to repository content checks.
 * Symbols and line locations: see docs/CODE_INDEX.md.
 * Important variables: limits bound descriptor reads, ZIP members and inflated text; no content executes.
 */
import { closeSync, constants, fstatSync, openSync, readSync } from "node:fs";
import { inflateRawSync } from "node:zlib";

const maxBytes = 8 * 1024 * 1024;
const maxXmlBytes = 2 * 1024 * 1024;
const utf8 = new TextDecoder("utf-8", { fatal: true });

/** Reject malformed package claims with an active guard, including optimized Node execution. */
function requireValue(condition, message) {
  if (!condition) throw new Error(`Reviewed publication: ${message}`);
}

/** Read one opened descriptor up to a fixed cap, retaining the original file identity. */
function boundedBytes(path) {
  const descriptor = openSync(
    path,
    constants.O_RDONLY | (constants.O_NONBLOCK ?? 0),
  );
  try {
    const status = fstatSync(descriptor);
    requireValue(
      status.isFile(),
      "only a regular source file can be inspected",
    );
    requireValue(status.size <= maxBytes, "file exceeds byte budget");
    const buffer = Buffer.alloc(maxBytes + 1);
    let count = 0;
    while (count < buffer.length) {
      const received = readSync(
        descriptor,
        buffer,
        count,
        buffer.length - count,
        count,
      );
      if (!received) break;
      count += received;
    }
    requireValue(count <= maxBytes, "file grew beyond byte budget");
    return buffer.subarray(0, count);
  } finally {
    closeSync(descriptor);
  }
}

/** Compute standard ZIP CRC-32 for each fully decoded member, including non-XML parts. */
function crc32(bytes) {
  let value = 0xffffffff;
  for (const byte of bytes) {
    value ^= byte;
    for (let bit = 0; bit < 8; bit += 1)
      value = (value >>> 1) ^ (value & 1 ? 0xedb88320 : 0);
  }
  return (value ^ 0xffffffff) >>> 0;
}

/** Inspect a conventional single-disk ZIP, rejecting ambiguity, active payloads and expansion abuse. */
function wordText(bytes) {
  let end = -1;
  for (
    let offset = bytes.length - 22;
    offset >= Math.max(0, bytes.length - 65557);
    offset -= 1
  ) {
    if (
      bytes.readUInt32LE(offset) === 0x06054b50 &&
      offset + 22 + bytes.readUInt16LE(offset + 20) === bytes.length
    ) {
      end = offset;
      break;
    }
  }
  requireValue(end >= 0, "missing ZIP end record");
  const count = bytes.readUInt16LE(end + 10);
  let cursor = bytes.readUInt32LE(end + 16);
  const directoryEnd = cursor + bytes.readUInt32LE(end + 12);
  requireValue(
    bytes.readUInt16LE(end + 4) === 0 &&
      bytes.readUInt16LE(end + 6) === 0 &&
      bytes.readUInt16LE(end + 8) === count,
    "split ZIP is unsupported",
  );
  requireValue(
    count > 0 && count <= 256 && directoryEnd === end,
    "directory identity or member budget",
  );
  const names = new Set();
  const ranges = [];
  const text = [];
  let expanded = 0;
  let xmlBytes = 0;
  for (let index = 0; index < count; index += 1) {
    requireValue(
      cursor + 46 <= directoryEnd && bytes.readUInt32LE(cursor) === 0x02014b50,
      "invalid directory member",
    );
    const flags = bytes.readUInt16LE(cursor + 8);
    const method = bytes.readUInt16LE(cursor + 10);
    const expectedCrc = bytes.readUInt32LE(cursor + 16);
    const compressed = bytes.readUInt32LE(cursor + 20);
    const size = bytes.readUInt32LE(cursor + 24);
    const nameLength = bytes.readUInt16LE(cursor + 28);
    const next =
      cursor +
      46 +
      nameLength +
      bytes.readUInt16LE(cursor + 30) +
      bytes.readUInt16LE(cursor + 32);
    requireValue(
      next <= directoryEnd && bytes.readUInt16LE(cursor + 34) === 0,
      "invalid directory extent",
    );
    const nameBytes = bytes.subarray(cursor + 46, cursor + 46 + nameLength);
    const name = utf8.decode(nameBytes);
    requireValue(
      name.length > 0 &&
        !names.has(name) &&
        !name.startsWith("/") &&
        !name.includes("\\") &&
        !name.split("/").includes(".."),
      "ambiguous member name",
    );
    requireValue(
      !(flags & 9) &&
        [0, 8].includes(method) &&
        !/vbaproject|\/embeddings\//i.test(name),
      "active, encrypted or unsupported member",
    );
    names.add(name);
    expanded += size;
    requireValue(
      expanded <= maxBytes && size <= maxXmlBytes,
      "expanded member budget",
    );
    const local = bytes.readUInt32LE(cursor + 42);
    requireValue(
      local + 30 <= bytes.readUInt32LE(end + 16) &&
        bytes.readUInt32LE(local) === 0x04034b50,
      "invalid local member",
    );
    const localNameEnd = local + 30 + bytes.readUInt16LE(local + 26);
    const start = localNameEnd + bytes.readUInt16LE(local + 28);
    const finish = start + compressed;
    requireValue(
      bytes.readUInt16LE(local + 6) === flags &&
        bytes.readUInt16LE(local + 8) === method &&
        bytes.readUInt32LE(local + 14) === expectedCrc &&
        bytes.readUInt32LE(local + 18) === compressed &&
        bytes.readUInt32LE(local + 22) === size &&
        localNameEnd <= start &&
        finish <= bytes.readUInt32LE(end + 16),
      "local member disagrees with directory",
    );
    requireValue(
      bytes.subarray(local + 30, localNameEnd).equals(nameBytes) &&
        ranges.every(([a, b]) => finish <= a || local >= b),
      "aliased or overlapping member",
    );
    ranges.push([local, finish]);
    const encoded = bytes.subarray(start, finish);
    const payload =
      method === 8
        ? inflateRawSync(encoded, { maxOutputLength: maxXmlBytes })
        : encoded;
    requireValue(
      payload.length === size && crc32(payload) === expectedCrc,
      "member size or CRC mismatch",
    );
    if (name.endsWith(".xml") || name.endsWith(".rels")) {
      xmlBytes += payload.length;
      requireValue(xmlBytes <= maxXmlBytes, "XML inspection budget");
      text.push(utf8.decode(payload));
    }
    cursor = next;
  }
  requireValue(
    cursor === directoryEnd &&
      names.has("[Content_Types].xml") &&
      names.has("word/document.xml"),
    "required Word parts missing",
  );
  return text.join("\n");
}

/** Decode ordinary source strictly; only the exact reviewed edition receives bounded ZIP handling. */
export function publicationText(path, repositoryPath) {
  const bytes = boundedBytes(path);
  return ["docs/reports/project-guide.docx", "docs/reports/windows-application-guide-1.1.0.docx"].includes(repositoryPath)
    ? wordText(bytes)
    : utf8.decode(bytes);
}
