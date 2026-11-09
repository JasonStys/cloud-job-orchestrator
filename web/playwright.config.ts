/** File: Jason Stys: owned local dashboard contract journeys, no server reuse or retries. */
import { defineConfig, devices } from "@playwright/test";
const port = Number(process.env.CJO_WEB_TEST_PORT ?? 18933);
if (!Number.isSafeInteger(port) || port < 1024 || port > 65535)
  throw new Error("Invalid test port");
export default defineConfig({
  testDir: "browser",
  workers: 1,
  retries: 0,
  timeout: 30000,
  outputDir: "test-results",
  reporter: "list",
  use: { baseURL: `http://127.0.0.1:${port}` },
  webServer: {
    command: "node scripts/test-server.mjs",
    url: `http://127.0.0.1:${port}/`,
    reuseExistingServer: false,
    timeout: 15000,
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "firefox", use: { ...devices["Desktop Firefox"] } },
    { name: "webkit", use: { ...devices["Desktop Safari"] } },
    { name: "mobile", use: { ...devices["iPhone 13"] } },
  ],
});
