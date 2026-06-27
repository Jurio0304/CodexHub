---
name: codexhub-maintainer
description: Use this project skill when developing, reviewing, testing, or maintaining CodexHub, a Windows-first Tauri 2 + React + TypeScript + Rust desktop control plane for Codex App SSH multi-server workflows. Trigger for changes touching README/docs, src, src-tauri, scripts, SSH config handling, settings persistence, mock/desktop backend boundaries, profile or skill sync flows, safety gates, or release readiness.
---

# CodexHub 项目维护指南

## 项目定位

CodexHub 是 Windows-first 桌面控制面板，用于安全管理 Codex App 的 SSH 多服务器工作流。MVP 以 Tauri 2 + React + TypeScript + Vite + Rust 实现，通过 Windows OpenSSH/SSH/SFTP 管理远端 Codex 配置和技能目录，而不是替代 Codex App。

当前进展：Window 2 已有桌面 UI 壳、本地外观设置持久化、本地 SSH key 状态检测、非覆盖式 Ed25519 key 生成，以及 `%USERPROFILE%\.ssh\config` 中 CodexHub 托管与本地 Host 块的统一增删改查。主机页已经收敛远端 Codex 探测、安装、更新入口；配置页保留为空壳。远端 SSH/SFTP 配置读写、profile apply、skill sync 仍以 mock/预留命令为主。

## 开发优先级

1. 先保护用户环境，再实现功能：任何写入 SSH config、远端 `~/.codex/config.toml`、远端 `~/.codex/skills/` 的路径都必须可预览、可备份、可重复执行、可恢复。
2. 继续沿用直接 SSH/SFTP 管理路线：MVP 不依赖远端 wrapper，不写 Codex App 私有状态，不假设存在未公开的 host 注册或 reconnect API。
3. 优先让 Windows 本地开发可验证：`pnpm dev:mock` 和 `pnpm smoke` 应保持低依赖；完整桌面用 `pnpm dev`。
4. UI、类型、后端命令一起演进：新增 Tauri command 时同步更新 Rust serde 结构、`src/models.ts`、`src/api.ts` fallback、React 调用和 smoke/test 覆盖。
5. 保持窄 diff：延续现有 macOS-style sidebar、卡片、表格、浅/深色变量和英/中 copy 结构，避免无关重构。
6. 开发只在临时分支进行；主分支只用于稳定基线，必须等用户确认后再合并。

## 安全边界

- 不读取、显示、存储 SSH 私钥、passphrase、OpenAI token 或远端 secret；UI 只可返回/复制 public key。
- 不整体覆盖 `%USERPROFILE%\.ssh\config`；写入前创建 timestamped backup，只精确修改目标 Host 块或目标 alias。
- CodexHub 托管块继续使用 `# >>> CodexHub managed host: <alias>` 到 `# <<< CodexHub managed host: <alias>` 标记；本地非托管块可统一编辑/删除，但必须保留无关内容、注释和其他 Host。
- 本地单别名 Host 块可整块替换或删除；本地多别名 Host 块编辑时拆出目标 alias 的独立 Host 块，删除时只移除目标 alias。
- 修改既有本地或远端文件前创建 timestamped backup；内容未变时报告 no changes，不制造新备份。
- 操作日志和错误信息默认去除 key material、token、passphrase；必要时再考虑用户名/主机名脱敏。
- Codex App 集成只能给出 Settings > Codex > Connections 的手动引导，除非后续有公开稳定 API。

## 分支与合并边界

- 开始编码前检查 `git status --short --branch`；若在 `main`、`master` 或其他稳定分支上，先创建临时开发分支，例如 `codex/<task-slug>`。
- 不要直接在主分支提交功能、修复或文档改动；除非用户明确要求热修或直接主分支操作。
- 不要自行执行 merge、rebase 到主分支、push 主分支或删除开发分支；先汇报改动范围、验证结果和风险，等待用户确认。
- 用户确认合并后，再按其指定方式合并；如未指定，优先保持线性、可回滚的小提交。
- 若发现主分支已有用户未提交改动，立即停止并询问，不要 stash、reset、checkout 或覆盖。

## 代码约定

- 前端类型集中在 `src/models.ts`，Tauri 调用和 web/mock fallback 集中在 `src/api.ts`，设置归一化和本地 fallback 在 `src/settings.ts`。
- `safeInvoke` 用于可降级读取/mock 操作；真正会写入系统或需要桌面后端的操作用 `requiredInvoke`，让错误显式暴露给 UI。
- Rust 命令使用 `#[tauri::command]` 暴露，serde 字段保持 `camelCase` 与 TypeScript 类型一致；枚举命名要和前端 union 对齐。
- SSH config 逻辑放在 `src-tauri/src/ssh.rs`；解析、幂等更新、拒绝非托管冲突和 backup 行为必须有 Rust 单元测试。
- UI 文案使用 `src/App.tsx` 的 `uiCopy.en` / `uiCopy.zh` 双语结构；新增页面或按钮时同步两种语言。
- CSS 延续 `src/styles.css` 的变量体系和响应式断点；不要引入新的视觉系统，除非用户明确要求。

## 常用验证

优先按改动范围选择最小验证集：

```powershell
pnpm smoke
pnpm typecheck
pnpm build:web
cd src-tauri; cargo test
```

如果本机 `node` 或 `pnpm` 不可用，优先使用 Codex 桌面提供的 bundled Node/pnpm 路径。完整 Tauri 桌面验证需要 Node 20+、pnpm、Rust stable MSVC、WebView2 和 Windows OpenSSH。

不要打开 Codex App 内置浏览器做视觉或运行时检查：当前内置浏览器存在打开后导致 Codex App 闪退的已知问题。优先使用静态断言、CLI 验证、用户提供截图，必要时先征得用户同意再用外部浏览器或无头 Playwright。

## 维护流程

1. 阅读 `README.md`、`docs/architecture.md`、`docs/mvp-scope.md`、`docs/known-limitations.md` 中与任务相关的部分。
2. 运行或检查 `git status --short --branch`，不要覆盖用户已有改动。
3. 确认当前工作位于临时开发分支；如需从主分支切出，先保证工作区干净或得到用户明确指示。
4. 做最小实现；涉及文件写入时先补安全门禁和测试，再接 UI。
5. 更新 README 或 docs 时只写当前事实：区分 mock、已连真实桌面后端、尚未实现的远端能力。
6. 结束前说明实际验证命令、结果、当前分支和是否等待用户确认合并；若只验证了 web/mock，不要声称完整桌面或远端流程已通过。
