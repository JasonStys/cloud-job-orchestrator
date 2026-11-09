/** File: Serve exactly five compiled dashboard assets on an owned loopback test listener. No proxy, uploads or writes. */
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
const port = Number(process.env.CJO_WEB_TEST_PORT ?? 18933);
if (!Number.isSafeInteger(port) || port < 1024 || port > 65535)
  throw new Error("Invalid test port");
const assets = new Map([
  ["/", ["index.html", "text/html"]],
  ["/app.js", ["app.js", "text/javascript"]],
  ["/api.js", ["api.js", "text/javascript"]],
  ["/model.js", ["model.js", "text/javascript"]],
  ["/styles.css", ["styles.css", "text/css"]],
]);
const server = createServer(async (request, response) => {
  const entry = request.method === "GET" ? assets.get(request.url) : undefined;
  if (!entry) {
    response.writeHead(404);
    response.end();
    return;
  }
  try {
    const bytes = await readFile(
      new URL(`../dist/${entry[0]}`, import.meta.url),
    );
    response.writeHead(200, {
      "content-type": `${entry[1]}; charset=utf-8`,
      "x-content-type-options": "nosniff",
    });
    response.end(bytes);
  } catch {
    response.writeHead(500);
    response.end("Build test assets first");
  }
});
server.listen(port, "127.0.0.1");
