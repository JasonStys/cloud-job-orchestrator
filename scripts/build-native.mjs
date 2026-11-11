/** File: scripts/build-native.mjs
 * Purpose: Package the genuine Rust/PostgreSQL/TypeScript application with a bundled Windows shell.
 * Functions: sourceSnapshot and inventory bind public files; checked runs fixed bounded build tools.
 * State: explicit verified SDK inputs/fresh generated outputs only; never install or open user data.
 */
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { createHash, randomBytes } from "node:crypto";
import {
  cp,
  mkdir,
  readdir,
  lstat,
  readFile,
  writeFile,
} from "node:fs/promises";
import { dirname, join, resolve, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

const { readRegular } = createRequire(import.meta.url)("../desktop/files.cjs");

const run = promisify(execFile);
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const output = resolve(
  root,
  process.env.CJO_NATIVE_OUTPUT ?? "release/pilot-001",
);
const postgres = resolve(process.env.CJO_POSTGRES_SDK ?? "");
const resources = join(
  root,
  ".runtime/native-resources-" + randomBytes(8).toString("hex"),
);
const VERSION = "1.1.0";
const excluded = new Set([
  ".git",
  ".runtime",
  "node_modules",
  "target",
  "release",
  "dist",
  "test-results",
  "__pycache__",
  "coverage",
]);

async function inventory(directory, target = directory) {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name),
      info = await lstat(path);
    if (
      target === root &&
      (excluded.has(entry.name) ||
        relative(root, path).replaceAll("\\", "/") === "artifacts/native")
    )
      continue;
    if (
      info.isSymbolicLink() ||
      (!info.isDirectory() && (!info.isFile() || info.nlink !== 1))
    )
      throw new Error("Linked or special source input refused.");
    if (info.isDirectory()) files.push(...(await inventory(path, target)));
    else {
      if (info.size > 32 * 1024 * 1024)
        throw new Error("Build input exceeds its bound.");
      files.push(path);
    }
  }
  if (files.length > 5000)
    throw new Error("Build inventory exceeds its bound.");
  return files;
}
async function fingerprint(directory) {
  const rows = {};
  const files =
    directory === root
      ? (
          await checked("git", [
            "-c",
            `safe.directory=${root.replaceAll("\\", "/")}`,
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
          ])
        )
          .split("\0")
          .filter(Boolean)
          .map((name) => resolve(root, name))
      : await inventory(directory);
  if (files.length > 5000)
    throw new Error("Source inventory exceeds its bound.");
  for (const path of files.sort()) {
    if (!path.startsWith(directory + sep))
      throw new Error("Source file escaped its root.");
    for (
      let parent = dirname(path);
      parent !== directory;
      parent = dirname(parent)
    ) {
      const info = await lstat(parent);
      if (!info.isDirectory() || info.isSymbolicLink())
        throw new Error("Linked source ancestor refused.");
    }
    rows[relative(directory, path).replaceAll("\\", "/")] = createHash("sha256")
      .update(readRegular(path, (directory === root ? 32 : 64) * 1024 * 1024))
      .digest("hex");
  }
  return rows;
}
async function checked(executable, arguments_, options = {}) {
  return (
    await run(executable, arguments_, {
      cwd: root,
      windowsHide: true,
      encoding: "utf8",
      timeout: 180_000,
      maxBuffer: 2 * 1024 * 1024,
      ...options,
    })
  ).stdout;
}
async function git(...arguments_) {
  return (
    await checked("git", [
      "-c",
      `safe.directory=${root.replaceAll("\\", "/")}`,
      ...arguments_,
    ])
  ).trim();
}

/** Author a distinct small dependency-graph icon using standard ICO/BMP bytes only. */
async function writeIcon(path) {
  const side = 256,
    pixels = Buffer.alloc(side * side * 4);
  for (let y = 0; y < side; y++)
    for (let x = 0; x < side; x++) {
      const u = Math.floor(x / 4),
        v = Math.floor(y / 4);
      const tile = u > 2 && u < 62 && v > 2 && v < 62;
      const nodes =
        (u >= 10 && u <= 23 && v >= 25 && v <= 39) ||
        (u >= 41 && u <= 54 && ((v >= 10 && v <= 23) || (v >= 42 && v <= 55)));
      const edges =
        (u >= 23 && u <= 35 && v >= 30 && v <= 34) ||
        (u >= 32 && u <= 36 && v >= 15 && v <= 50) ||
        (u >= 34 && u <= 42 && ((v >= 15 && v <= 19) || (v >= 47 && v <= 51)));
      Buffer.from(
        nodes || edges
          ? [188, 231, 87, 255]
          : tile
            ? [39, 53, 12, 255]
            : [0, 0, 0, 0],
      ).copy(pixels, ((side - 1 - y) * side + x) * 4);
    }
  const bitmap = Buffer.alloc(40);
  bitmap.writeUInt32LE(40, 0);
  bitmap.writeInt32LE(side, 4);
  bitmap.writeInt32LE(side * 2, 8);
  bitmap.writeUInt16LE(1, 12);
  bitmap.writeUInt16LE(32, 14);
  bitmap.writeUInt32LE(pixels.length, 20);
  const image = Buffer.concat([
      bitmap,
      pixels,
      Buffer.alloc((side * side) / 8),
    ]),
    header = Buffer.alloc(22);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(1, 4); // Zero width/height encodes256pixels in ICO.
  header.writeUInt16LE(1, 10);
  header.writeUInt16LE(32, 12);
  header.writeUInt32LE(image.length, 14);
  header.writeUInt32LE(22, 18);
  await writeFile(path, Buffer.concat([header, image]), { flag: "wx" });
}

if (process.platform !== "win32" || process.arch !== "x64")
  throw new Error("Current native package targets Windows x64.");
if (!output.startsWith(join(root, "release") + sep))
  throw new Error("Fresh native output must remain under release/.");
await mkdir(dirname(output), { recursive: true });
await mkdir(output);
const source = await git("rev-parse", "HEAD");
const sourceDirty = Boolean(await git("status", "--porcelain"));
if (
  !/^[0-9a-f]{40}$/.test(source) ||
  (process.env.GITHUB_ACTIONS === "true" &&
    (sourceDirty || process.env.GITHUB_SHA !== source))
)
  throw new Error("Hosted native build requires exact clean workflow source.");
const snapshot = await fingerprint(root);
const sdk = JSON.parse(
  readRegular(join(postgres, "SDK.json"), 512 * 1024).toString("utf8"),
);
if (
  sdk.schema !== 2 ||
  sdk.version !== "18.6" ||
  sdk.archive_sha256 !==
    "e2246ba91d22345bc3d017586c09ede52d9df180b1eeb480f050445f1cad84e2" ||
  Object.keys(sdk.files).length !== 1485 ||
  Object.keys(sdk.crt_files).length !== 5
)
  throw new Error("Verified native PostgreSQL/CRT source is required.");
for (const [name, digest] of Object.entries({
  ...sdk.files,
  ...Object.fromEntries(
    Object.entries(sdk.crt_files).map(([name, value]) => [
      "bin/" + name,
      value,
    ]),
  ),
})) {
  if (
    name.includes("\\") ||
    name.includes(":") ||
    name.split("/").some((part) => !part || part === "." || part === "..")
  )
    throw new Error("Invalid provider inventory path.");
  const path = join(postgres, ...name.split("/"));
  if (
    createHash("sha256")
      .update(readRegular(path, 64 * 1024 * 1024))
      .digest("hex") !== digest
  )
    throw new Error(
      "Actual PostgreSQL runtime differs from verified inventory.",
    );
}
await checked(
  "cargo",
  ["build", "--release", "--locked", "--bin", "orchestrator-native"],
  {
    timeout: 600_000,
    env: {
      ...process.env,
      CJO_BUILD_SOURCE: source,
      CJO_BUILD_DIRTY: String(sourceDirty),
    },
  },
);
await checked(process.execPath, [
  join(root, "web/node_modules/typescript/bin/tsc"),
  "--project",
  "web/tsconfig.json",
]);
await checked(process.execPath, ["web/scripts/build.mjs"]);
// Never reset staging: every candidate is separately preserved, including failed builds.
await mkdir(resources);
await mkdir(join(resources, "bin"));
await cp(
  join(root, "target/release/orchestrator-native.exe"),
  join(resources, "bin/orchestrator-native.exe"),
  { errorOnExist: true, force: false },
);
await cp(postgres, join(resources, "postgres"), {
  recursive: true,
  errorOnExist: true,
  force: false,
});
for (const name of Object.keys(sdk.crt_files))
  await cp(join(postgres, "bin", name), join(resources, "bin", name), {
    errorOnExist: true,
    force: false,
  });
await cp(join(root, "web/dist"), join(resources, "web/dist"), {
  recursive: true,
  errorOnExist: true,
  force: false,
});
await mkdir(join(resources, "notices"));
await cp(join(root, "LICENSE"), join(resources, "notices/PROJECT-LICENSE.txt"));
await cp(join(root, "Cargo.lock"), join(resources, "notices/Cargo.lock"));
await cp(
  join(root, "desktop/package-lock.json"),
  join(resources, "notices/desktop-package-lock.json"),
);
const rustRoot = (await checked("rustc", ["--print", "sysroot"])).trim();
const libraryCopyright = join(
  rustRoot,
  "share/doc/rust/COPYRIGHT-library.html",
);
const copyrightInfo = await lstat(libraryCopyright);
if (
  !copyrightInfo.isFile() ||
  copyrightInfo.isSymbolicLink() ||
  copyrightInfo.size > 2 * 1024 * 1024
)
  throw new Error("Actual Rust standard-library notices are required.");
await cp(
  libraryCopyright,
  join(resources, "notices/RUST-COPYRIGHT-library.html"),
);
await writeFile(
  join(resources, "notices/MICROSOFT-RUNTIME.txt"),
  "Unmodified Microsoft Corporation C++ Runtime files, licensed separately from this project's MIT source.\nhttps://learn.microsoft.com/en-us/visualstudio/releases/2026/redistribution\nhttps://visualstudio.microsoft.com/license-terms/vs2026-ga-community/\n",
  { flag: "wx" },
);
const dependencies = JSON.parse(
  await checked("cargo", [
    "metadata",
    "--locked",
    "--format-version",
    "1",
    "--filter-platform",
    "x86_64-pc-windows-msvc",
  ]),
);
const nodes = new Map(
  dependencies.resolve.nodes.map((value) => [value.id, value]),
);
const runtimeIds = new Set(),
  pending = [dependencies.resolve.root];
while (pending.length) {
  const id = pending.pop();
  if (runtimeIds.has(id)) continue;
  runtimeIds.add(id);
  for (const dependency of nodes.get(id).deps)
    if (dependency.dep_kinds.some((kind) => kind.kind === null))
      pending.push(dependency.pkg);
}
const notices = [];
for (const dependency of dependencies.packages.filter(
  (value) => value.source !== null && runtimeIds.has(value.id),
)) {
  const base = dirname(dependency.manifest_path);
  const names = (await readdir(base)).filter((name) =>
    /^(?:licen[cs]es?|copying|notice|copyright)(?:[-_.]|$)/i.test(name),
  );
  if (!names.length || !dependency.license)
    throw new Error(
      `Actual upstream license text required for ${dependency.name}@${dependency.version}`,
    );
  const destination = join(
    resources,
    "notices/rust",
    `${dependency.name}-${dependency.version}`,
  );
  await mkdir(destination, { recursive: true });
  let copied = 0;
  for (const name of names) {
    const path = join(base, name),
      info = await lstat(path);
    if (info.isSymbolicLink()) throw new Error("Linked license input refused.");
    if (info.isDirectory()) {
      const children = await readdir(path);
      if (children.length > 16)
        throw new Error("License directory exceeds its bound.");
      await mkdir(join(destination, name));
      for (const child of children) {
        const source = join(path, child),
          details = await lstat(source);
        if (
          !details.isFile() ||
          details.isSymbolicLink() ||
          details.size > 256 * 1024
        )
          throw new Error("Actual license member is invalid.");
        await cp(source, join(destination, name, child));
        copied += 1;
      }
    } else {
      if (!info.isFile() || info.size > 256 * 1024)
        throw new Error("License input exceeds its bound.");
      await cp(path, join(destination, name));
      copied += 1;
    }
  }
  if (!copied)
    throw new Error("Actual dependency license files were unavailable.");
  notices.push({
    name: dependency.name,
    version: dependency.version,
    license: dependency.license,
  });
}
await writeFile(
  join(resources, "notices/rust-dependencies.json"),
  JSON.stringify(notices, null, 2) + "\n",
  { flag: "wx" },
);
const metadata = {
  schema: 1,
  product: "cloud-job-orchestrator",
  version: VERSION,
  source,
  sourceDirty,
  postgres: "18.6",
  electron: "44.11.0",
  files: await fingerprint(resources),
  asar: Object.fromEntries(
    Object.entries(snapshot).filter(([name]) =>
      /^desktop\/[^/]+\.cjs$/.test(name),
    ),
  ),
};
await writeFile(
  join(resources, "SOURCE.json"),
  JSON.stringify(metadata, null, 2) + "\n",
  { flag: "wx" },
);
if (JSON.stringify(await fingerprint(root)) !== JSON.stringify(snapshot))
  throw new Error("Authored source changed during native build.");
const environment = {
  ...process.env,
  ELECTRON_BUILDER_ALLOW_UNRESOLVED_DEPENDENCIES: "false",
  CSC_IDENTITY_AUTO_DISCOVERY: "false",
};
const configuration = JSON.parse(
  await readFile(join(root, "desktop/package.json"), "utf8"),
).build;
configuration.directories.output = output;
configuration.extraResources = [{ from: resources, to: "native" }];
configuration.win.icon = resources + ".ico";
await writeIcon(configuration.win.icon);
const configPath = resources + "-builder.json";
await writeFile(configPath, JSON.stringify(configuration, null, 2), {
  flag: "wx",
});
await checked(
  process.execPath,
  [
    join(root, "desktop/node_modules/electron-builder/cli.js"),
    "--projectDir",
    "desktop",
    "--win",
    "dir",
    "--publish",
    "never",
    "--config",
    configPath,
  ],
  { timeout: 300_000, env: environment },
);
if (JSON.stringify(await fingerprint(root)) !== JSON.stringify(snapshot))
  throw new Error("Source changed during shell packaging.");
console.log(
  JSON.stringify({
    built: true,
    source,
    sourceDirty,
    version: VERSION,
    output,
  }),
);
