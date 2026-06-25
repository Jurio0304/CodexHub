import http from "node:http";

const port = Number(process.env.CODEXHUB_MOCK_PORT ?? 4173);

const html = `<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>CodexHub Mock</title>
  <style>
    body { margin: 0; min-height: 100vh; display: grid; place-items: center; font-family: Segoe UI, sans-serif; background: linear-gradient(135deg, #f8fafc, #dfe7f0); color: #172033; }
    main { width: min(760px, calc(100% - 32px)); padding: 40px; border-radius: 28px; background: rgba(255,255,255,.75); box-shadow: 0 24px 80px rgba(36,50,74,.15); }
    h1 { margin: 0 0 12px; font-size: clamp(44px, 9vw, 88px); letter-spacing: -.07em; line-height: .9; }
    p { margin: 0; color: #51607a; line-height: 1.6; }
    code { background: rgba(23,32,51,.08); padding: 2px 6px; border-radius: 6px; }
  </style>
</head>
<body>
  <main>
    <h1>CodexHub</h1>
    <p>Mock mode is running. The MVP will manage <code>~/.codex/config.toml</code> and <code>~/.codex/skills/</code> through explicit SSH/SFTP operations.</p>
  </main>
</body>
</html>`;

const server = http.createServer((request, response) => {
  if (request.url === "/health") {
    response.writeHead(200, { "content-type": "application/json; charset=utf-8" });
    response.end(JSON.stringify({ app: "CodexHub", mode: "mock", ok: true }));
    return;
  }

  response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
  response.end(html);
});

server.listen(port, "127.0.0.1", () => {
  console.log(`CodexHub mock mode: http://127.0.0.1:${port}`);
});
