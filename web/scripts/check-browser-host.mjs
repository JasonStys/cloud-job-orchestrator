/** File: check-browser-host.mjs; Purpose: Jason Stys: prove each real locked browser and missing-executable refusal before original app journeys. Variables: requested engines, installed graph and owned pages; no profile/provider/OS package changes. */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { chromium, firefox, webkit } from "@playwright/test";

const engines = { chromium, firefox, webkit };
const requested = process.argv.slice(2);
assert(requested.length > 0 && new Set(requested).size === requested.length);
assert(requested.every((name) => Object.hasOwn(engines, name)));
assert(
  !Object.hasOwn(process.env, "PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS"),
);
const lock = JSON.parse(await fs.readFile("package-lock.json", "utf8"));
const lookup = createRequire(import.meta.url);
const testPackage = lookup.resolve("@playwright/test/package.json");
const playwrightPackage = createRequire(testPackage).resolve(
  "playwright/package.json",
);
const driverPackage = createRequire(playwrightPackage).resolve(
  "playwright-core/package.json",
);
const installed = JSON.parse(await fs.readFile(driverPackage, "utf8"));
const driverLockPath = path
  .relative(process.cwd(), path.dirname(driverPackage))
  .replaceAll("\\", "/");
const declared = JSON.parse(await fs.readFile(playwrightPackage, "utf8"))
  .dependencies["playwright-core"];
assert.equal(installed.version, lock.packages[driverLockPath]?.version);
assert.equal(installed.version, declared);
const temporary = await fs.mkdtemp(
  path.join(os.tmpdir(), "portfolio-browser-host-"),
);
try {
  for (const name of requested) {
    const engine = engines[name];
    if (process.env.GITHUB_ACTIONS === "true") {
      assert.equal(process.getuid(), 1001);
      assert(engine.executablePath().startsWith("/ms-playwright/"));
    }
    // A real missing executable must fail. Never accept a caught failure of the
    // subsequent ordinary launch, and never bypass Playwright's host validation.
    await assert.rejects(
      engine.launch({ executablePath: path.join(temporary, "absent-browser") }),
      /executable doesn't exist|Executable doesn't exist/i,
    );
    const browser = await engine.launch();
    try {
      const page = await browser.newPage();
      let outsideRequests = 0;
      await page.route("**/*", (route) => {
        outsideRequests += 1;
        return route.abort();
      });
      await page.setContent(
        '<main><label for="entry">Résumé — 123 mm</label><input id="entry"></main>',
      );
      await page.locator("#entry").fill("host dependency probe");
      assert.equal(
        await page.locator("#entry").inputValue(),
        "host dependency probe",
      );
      assert.equal(await page.locator("label").innerText(), "Résumé — 123 mm");
      const layouts = [];
      for (const width of [390, 1400]) {
        await page.setViewportSize({ width, height: 900 });
        const layout = await page.evaluate(() => {
          const context = document.createElement("canvas").getContext("2d");
          if (!context) throw new Error("Actual font rendering is unavailable");
          context.font = "16px sans-serif";
          return {
            width: innerWidth,
            documentWidth: document.documentElement.scrollWidth,
            narrow: context.measureText("iiii").width,
            wide: context.measureText("WWWW").width,
            fontReady: document.fonts.check("16px sans-serif"),
          };
        });
        assert.equal(layout.width, width);
        assert(
          layout.documentWidth <= width &&
            layout.fontReady &&
            layout.narrow > 0 &&
            layout.wide > layout.narrow,
        );
        await page.locator("#entry").focus();
        assert(
          await page
            .locator("#entry")
            .evaluate((node) => node === document.activeElement),
        );
        layouts.push(layout);
      }
      assert.equal(outsideRequests, 0);
      console.log(
        JSON.stringify({
          browser: name,
          driverVersion: installed.version,
          version: browser.version(),
          launch: "passed",
          missingExecutable: "rejected",
          layouts,
          outsideRequests,
        }),
      );
    } finally {
      await browser.close();
    }
  }
} finally {
  // Remove only the one empty directory this probe created; no recursive cleanup.
  await fs.rmdir(temporary);
}
