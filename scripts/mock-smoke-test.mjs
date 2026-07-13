import { spawn } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import net from "node:net";
import path from "node:path";
import { build } from "vite";

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

const escapeRegExp = (value) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

const verifyMockBuild = async () => {
  await build({
    configFile: path.join(root, "vite.config.mjs"),
    logLevel: "silent",
    mode: "mock"
  });
  const assetDir = path.join(root, "dist", "assets");
  const bundles = fs.readdirSync(assetDir)
    .filter((name) => /^index-.*\.js$/u.test(name))
    .map((name) => fs.readFileSync(path.join(assetDir, name), "utf8"));
  if (bundles.length !== 1) fail(`expected one built JS bundle, found ${bundles.length}`);
  const bundle = bundles[0];
  if (bundle.includes("env?.MODE") || bundle.includes("env.MODE")) {
    fail("mock bundle still contains a runtime MODE lookup instead of a Vite build-time constant");
  }
  const marker = /"data-api-mode":([A-Za-z_$][\w$]*)/u.exec(bundle);
  if (!marker) fail("mock bundle is missing the rendered data-api-mode marker");
  const assignment = new RegExp(`(?:const|let|var)\\s+${escapeRegExp(marker[1])}=([A-Za-z_$][\\w$]*)\\(\\)`, "u").exec(bundle);
  if (!assignment) fail("mock bundle did not reduce the rendered API mode to a build-time resolver");
  const mockResolver = new RegExp(`function\\s+${escapeRegExp(assignment[1])}\\([^)]*\\)\\{return\"mock\"\\}`, "u");
  if (!mockResolver.test(bundle)) fail("mock bundle rendered a non-Mock API mode");
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
  await verifyMockBuild();
  const health = await waitForHealth(port);
  if (health.app !== "CodexHub" || health.mode !== "mock" || health.ok !== true) {
    fail(`unexpected health payload: ${JSON.stringify(health)}`);
  }
  const mockSource = fs.readFileSync(path.join(root, "src", "api", "mock.ts"), "utf8");
  const appSource = fs.readFileSync(path.join(root, "src", "App.tsx"), "utf8");
  const runtimeSource = fs.readFileSync(path.join(root, "src", "api", "runtime.ts"), "utf8");
  if (!runtimeSource.includes("resolveApiMode(import.meta.env.MODE)")) {
    fail("runtime API mode must use a direct Vite-replaceable import.meta.env.MODE access");
  }
  for (const token of [
    "runMockConcurrencyPool",
    "concurrency = 6",
    "results[index] = await worker(values[index], index)",
    "await Promise.all(runners)",
    "mockBatchRemoteProbeCodex",
    "mockBatchRemoteUpdateCodex",
    "mockRemoteProbeWithProgress",
    "mockRemoteManageCodexWithProgress",
    "batchRemoteProbeCodex:",
    "batchRemoteUpdateCodex:"
  ]) {
    if (!mockSource.includes(token)) fail(`mock host-operation flow is missing: ${token}`);
  }
  for (const stepId of [
    "preparation",
    "official-installer",
    "remote-native-mirror",
    "remote-npm-mirror",
    "local-upload",
    "final-verification"
  ]) {
    if (!mockSource.includes(`"${stepId}"`)) fail(`mock Codex stages are missing: ${stepId}`);
  }
  for (const token of [
    'stepId === "official-installer" || stepId === "remote-native-mirror"',
    'stepId === "local-upload") return "skipped"',
    'emit({ ...step, status: "running", endedAt: null })',
    'status: hostIndex < 6 ? "running" : "pending"',
    'status: sequence === 0 && hostIndex < 6 ? "running" : "pending"'
  ]) {
    if (!`${mockSource}\n${appSource}`.includes(token)) fail(`mock waiting/fallback progress is missing: ${token}`);
  }
  console.log(`MOCK SMOKE PASS: CodexHub mock mode is healthy on 127.0.0.1:${port}.`);
} catch (error) {
  fail(`${error.message}\n${output.trim()}`);
} finally {
  if (!child.killed) {
    child.kill();
  }
}
