/** File: Enforces documentation, source-header, workflow-pin, and portfolio-neutrality policies. Functions: walk and fail collect actionable violations. Variables: requiredDocs and sourceExtensions define the contract. */
import { access, readdir, readFile } from "node:fs/promises";
import { relative, resolve } from "node:path";

const repository = resolve(import.meta.dirname, "..");
const requiredDocs = ["README.md", "docs/ARCHITECTURE.md", "docs/API.md", "docs/DATA_MODEL.md", "docs/SCHEDULER.md", "docs/TESTING.md", "docs/SECURITY.md", "docs/OPERATIONS.md", "docs/DISASTER_RECOVERY.md", "docs/LIMITATIONS.md", "docs/RESEARCH.md", "docs/CODE_INDEX.md", "docs/reports/VALIDATION.md"];
const sourceExtensions = /\.(?:rs|ts|mjs|sh|bats|sql|tf|ya?ml|html|css)$/;
const excluded = new Set([".git", "node_modules", "target", "dist"]);
const problems = [];

async function walk(directory) {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (excluded.has(entry.name)) continue;
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) files.push(...await walk(path));
    else files.push(path);
  }
  return files;
}

for (const document of requiredDocs) {
  try { await access(resolve(repository, document)); } catch { problems.push(`missing required document: ${document}`); }
}

for (const file of await walk(repository)) {
  const path = relative(repository, file).replaceAll("\\", "/");
  if (sourceExtensions.test(path) || path.endsWith("Dockerfile")) {
    const firstLines = (await readFile(file, "utf8")).split(/\r?\n/).slice(0, 5).join("\n");
    if (!/File:/i.test(firstLines)) problems.push(`missing File header in ${path}`);
  }
  if (path.startsWith(".github/workflows/") && path.endsWith(".yml")) {
    const content = await readFile(file, "utf8");
    for (const line of content.split(/\r?\n/).filter((candidate) => /uses:\s*/.test(candidate))) {
      if (!/uses:\s*[\w.-]+\/[\w.-]+(?:\/[\w.-]+)?@[0-9a-f]{40}(?:\s+#\s*v?[0-9])/i.test(line)) {
        problems.push(`workflow action is not pinned to a full SHA: ${path}: ${line.trim()}`);
      }
    }
  }
}

const searchable = (await Promise.all((await walk(repository)).filter((file) => !file.includes("package-lock.json")).map((file) => readFile(file, "utf8").catch(() => "")))).join("\n");
if (/opto\s*22/i.test(searchable)) problems.push("portfolio-neutrality policy violation");
if (problems.length) {
  console.error(problems.join("\n"));
  process.exitCode = 1;
} else {
  console.log("repository policy validation passed");
}

