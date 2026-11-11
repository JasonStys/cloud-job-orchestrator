/**
 * File: publication-text.test.mjs
 * Purpose: Exercise real ZIP bytes, exact-path privacy visibility and malformed publication rejection.
 * Symbols and line locations: see docs/CODE_INDEX.md.
 * Important variables: fixture entries are inert synthetic content; scratch folders are removed after each test.
 */
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { deflateRawSync } from "node:zlib";
import { publicationText } from "./publication-text.mjs";

/** Produce a small standard stored ZIP independently from the production read/decompression path. */
function fixture(entries, deflated = false) {
  let offset = 0;
  const local = [];
  const central = [];
  for (const [name, value] of entries) {
    const label = Buffer.from(name);
    const payload = Buffer.isBuffer(value) ? value : Buffer.from(value);
    const encoded = deflated ? deflateRawSync(payload) : payload;
    let crc = -1;
    for (const byte of payload) {
      crc ^= byte;
      for (let bit = 0; bit < 8; bit += 1)
        crc = (crc >>> 1) ^ ((crc & 1) * 0xedb88320);
    }
    crc = (crc ^ -1) >>> 0;
    const header = Buffer.alloc(30);
    header.writeUInt32LE(0x04034b50);
    header.writeUInt16LE(deflated ? 8 : 0, 8);
    header.writeUInt32LE(crc, 14);
    header.writeUInt32LE(encoded.length, 18);
    header.writeUInt32LE(payload.length, 22);
    header.writeUInt16LE(label.length, 26);
    local.push(header, label, encoded);
    const row = Buffer.alloc(46);
    row.writeUInt32LE(0x02014b50);
    row.writeUInt16LE(deflated ? 8 : 0, 10);
    row.writeUInt32LE(crc, 16);
    row.writeUInt32LE(encoded.length, 20);
    row.writeUInt32LE(payload.length, 24);
    row.writeUInt16LE(label.length, 28);
    row.writeUInt32LE(offset, 42);
    central.push(row, label);
    offset += header.length + label.length + encoded.length;
  }
  const directory = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50);
  end.writeUInt16LE(entries.length, 8);
  end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(directory.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...local, directory, end]);
}

/** Use isolated bytes with no preexisting source/file replacement or real credential material. */
function withBytes(bytes, callback) {
  const folder = mkdtempSync(join(tmpdir(), "current-publication-"));
  try {
    const path = join(folder, "fixture");
    writeFileSync(path, bytes, { flag: "wx" });
    callback(path);
  } finally {
    rmSync(folder, { recursive: true, force: true });
  }
}

const base = [
  ["[Content_Types].xml", "<Types/>"],
  ["word/document.xml", "<document>Public guide.</document>"],
  [
    "word/_rels/document.xml.rels",
    "<Relationship>synthetic-private-marker</Relationship>",
  ],
];

test("the actual reviewed compressed Word bytes expose document and relationships", () => {
  const root = dirname(dirname(fileURLToPath(import.meta.url)));
  const text = publicationText(
    join(root, "docs/reports/project-guide.docx"),
    "docs/reports/project-guide.docx",
  );
  assert.match(text, /Jason Stys/);
  assert.match(text, /Relationships/);
});

test("stored fixture text and relationships remain available to the original content checks", () => {
  withBytes(fixture(base), (path) => {
    const text = publicationText(path, "docs/reports/project-guide.docx");
    assert.match(text, /Public guide/);
    assert.match(text, /synthetic-private-marker/);
  });
});

test("ordinary source remains strict UTF-8 and an unknown binary name is not an exclusion", () => {
  withBytes(Buffer.from("Public source."), (path) =>
    assert.equal(publicationText(path, "docs/source.md"), "Public source."),
  );
  withBytes(Buffer.concat([fixture(base), Buffer.from([255])]), (path) =>
    assert.throws(() => publicationText(path, "docs/reports/unreviewed.docx")),
  );
});

test("non-regular input is rejected without reading its contents", () => {
  const folder = mkdtempSync(join(tmpdir(), "current-publication-directory-"));
  try {
    assert.throws(() => publicationText(folder, "docs/source.md"));
  } finally {
    rmSync(folder, { recursive: true, force: true });
  }
});

test(
  "POSIX FIFO without a writer is rejected within the child deadline",
  { skip: process.platform === "win32" },
  () => {
    const folder = mkdtempSync(join(tmpdir(), "current-publication-fifo-"));
    try {
      const path = join(folder, "pipe");
      const created = spawnSync("mkfifo", [path], {
        encoding: "utf8",
        timeout: 2000,
      });
      assert.equal(created.status, 0, created.error?.message ?? created.stderr);
      const source = `import { publicationText } from ${JSON.stringify(new URL("./publication-text.mjs", import.meta.url).href)}; try { publicationText(process.argv[1], "docs/source.md"); process.exitCode = 1; } catch (error) { if (!String(error).includes("only a regular source file")) throw error; }`;
      const result = spawnSync(
        process.execPath,
        ["--input-type=module", "-e", source, path],
        { encoding: "utf8", timeout: 2000 },
      );
      assert.equal(result.status, 0, result.error?.message ?? result.stderr);
    } finally {
      rmSync(folder, { recursive: true, force: true });
    }
  },
);

test("independent conventional deflate ZIP exposes both XML and relationships", () => {
  withBytes(fixture(base, true), (path) => {
    const text = publicationText(path, "docs/reports/project-guide.docx");
    assert.match(text, /Public guide/);
    assert.match(text, /synthetic-private-marker/);
  });
});

test("deflated output cannot exceed its cap even when both headers understate its size", () => {
  const name = "word/expanded.xml";
  const bytes = fixture([...base, [name, "x".repeat(3 * 1024 * 1024)]], true);
  const local = bytes.indexOf(Buffer.from(name)) - 30;
  const central = bytes.lastIndexOf(Buffer.from(name)) - 46;
  bytes.writeUInt32LE(100, local + 22);
  bytes.writeUInt32LE(100, central + 24);
  withBytes(bytes, (path) =>
    assert.throws(() =>
      publicationText(path, "docs/reports/project-guide.docx"),
    ),
  );
});

test("CRC of fully inflated bytes is checked even when local and directory CRC agree", () => {
  const bytes = fixture(base, true);
  const central = bytes.readUInt32LE(bytes.length - 6);
  bytes.writeUInt32LE(0, 14);
  bytes.writeUInt32LE(0, central + 16);
  withBytes(bytes, (path) =>
    assert.throws(
      () => publicationText(path, "docs/reports/project-guide.docx"),
      /CRC mismatch/,
    ),
  );
});

for (const [label, entries] of [
  ["missing main", base.slice(0, 1)],
  ["duplicate member", [...base, base[1]]],
  ["macro", [...base, ["word/vbaProject.bin", "inert"]]],
  ["embedded payload", [...base, ["word/embeddings/inert.bin", "inert"]]],
  ["traversal", [...base, ["../private.xml", "inert"]]],
  [
    "too many members",
    [
      ...base,
      ...Array.from({ length: 254 }, (_, index) => [
        `word/part${index}.xml`,
        "<x/>",
      ]),
    ],
  ],
  [
    "XML expansion budget",
    [...base, ["word/large.xml", "x".repeat(2 * 1024 * 1024)]],
  ],
  ["invalid XML UTF-8", [...base, ["word/invalid.xml", Buffer.from([255])]]],
]) {
  test(`reject ${label}`, () =>
    withBytes(fixture(entries), (path) =>
      assert.throws(() =>
        publicationText(path, "docs/reports/project-guide.docx"),
      ),
    ));
}

for (const [label, corrupt] of [
  [
    "CRC mismatch",
    (bytes) => {
      bytes[30 + Buffer.byteLength(base[0][0])] ^= 1;
    },
  ],
  [
    "local method mismatch",
    (bytes) => {
      bytes.writeUInt16LE(8, 8);
    },
  ],
  [
    "encrypted member",
    (bytes) => {
      bytes.writeUInt16LE(1, 6);
    },
  ],
  [
    "local CRC mismatch",
    (bytes) => {
      bytes.writeUInt32LE(0, 14);
    },
  ],
  [
    "local compressed-size mismatch",
    (bytes) => {
      bytes.writeUInt32LE(0, 18);
    },
  ],
  [
    "local uncompressed-size mismatch",
    (bytes) => {
      bytes.writeUInt32LE(0, 22);
    },
  ],
  [
    "data descriptor",
    (bytes) => {
      bytes.writeUInt16LE(8, 6);
      bytes.writeUInt16LE(8, bytes.readUInt32LE(bytes.length - 6) + 8);
    },
  ],
  [
    "missing end record",
    (bytes) => {
      bytes.writeUInt32LE(0, bytes.length - 22);
    },
  ],
  [
    "invalid directory offset",
    (bytes) => {
      bytes.writeUInt32LE(0xffffffff, bytes.length - 6);
    },
  ],
]) {
  test(`reject ${label}`, () => {
    const bytes = fixture(base);
    corrupt(bytes);
    withBytes(bytes, (path) =>
      assert.throws(() =>
        publicationText(path, "docs/reports/project-guide.docx"),
      ),
    );
  });
}


test("current exact installed guide exposes XML and relationships", () => {
  const root = dirname(dirname(fileURLToPath(import.meta.url)));
  const text = publicationText(join(root, "docs/reports/windows-application-guide-1.1.0.docx"), "docs/reports/windows-application-guide-1.1.0.docx");
  assert.match(text, /Jason Stys/);
  assert.match(text, /Relationships/);
});
