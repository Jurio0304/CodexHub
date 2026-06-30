import { spawn } from "node:child_process";
import http from "node:http";
import net from "node:net";
import path from "node:path";

const root = process.cwd();

const fail = (message) => {
  console.error(`MOCK SMOKE FAIL: ${message}`);
  process.exit(1);
};

const findFreePort = () =>
  new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      const port = typeof address === "object" && address ? address.port : 0;
      server.close(() => resolve(port));
    });
  });

const getJson = (port, pathName) =>
  new Promise((resolve, reject) => {
    const request = http.get(
      {
        hostname: "127.0.0.1",
        port,
        path: pathName,
        timeout: 1_000
      },
      (response) => {
        let body = "";
        response.setEncoding("utf8");
        response.on("data", (chunk) => {
          body += chunk;
        });
        response.on("end", () => {
          if (response.statusCode !== 200) {
            reject(new Error(`HTTP ${response.statusCode}: ${body}`));
            return;
          }
          try {
            resolve(JSON.parse(body));
          } catch (error) {
            reject(error);
          }
        });
      }
    );
    request.on("timeout", () => {
      request.destroy(new Error("request timed out"));
    });
    request.on("error", reject);
  });

const waitForHealth = async (port) => {
  const deadline = Date.now() + 8_000;
  let lastError;
  while (Date.now() < deadline) {
    try {
      return await getJson(port, "/health");
    } catch (error) {
      lastError = error;
      await new Promise((resolve) => setTimeout(resolve, 150));
    }
  }
  throw lastError ?? new Error("mock server did not become healthy");
};

const port = await findFreePort();
const child = spawn(process.execPath, [path.join(root, "scripts", "mock-dev.mjs")], {
  cwd: root,
  env: {
    ...process.env,
    CODEXHUB_MOCK_PORT: String(port)
  },
  stdio: ["ignore", "pipe", "pipe"]
});

let output = "";
child.stdout.on("data", (chunk) => {
  output += chunk.toString();
});
child.stderr.on("data", (chunk) => {
  output += chunk.toString();
});

try {
  const health = await waitForHealth(port);
  if (health.app !== "CodexHub" || health.mode !== "mock" || health.ok !== true) {
    fail(`unexpected health payload: ${JSON.stringify(health)}`);
  }
  console.log(`MOCK SMOKE PASS: CodexHub mock mode is healthy on 127.0.0.1:${port}.`);
} catch (error) {
  fail(`${error.message}\n${output.trim()}`);
} finally {
  if (!child.killed) {
    child.kill();
  }
}
