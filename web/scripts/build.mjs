/** File: Copies reviewed static assets beside compiled TypeScript output. Functions: copyFile calls form the complete deterministic build. Variables: source and destination paths are repository-relative constants. */
import { copyFile, mkdir } from "node:fs/promises";

await mkdir(new URL("../dist/", import.meta.url), { recursive: true });
for (const asset of ["index.html", "styles.css"]) {
  await copyFile(new URL(`../public/${asset}`, import.meta.url), new URL(`../dist/${asset}`, import.meta.url));
}

