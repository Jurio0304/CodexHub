---
name: codexhub-maintainer
description: Use this project skill when developing, reviewing, testing, or maintaining CodexHub, a cross-platform Tauri 2 + React + TypeScript + Rust desktop control plane for Codex App SSH multi-server workflows. Trigger for changes touching README/docs, src, src-tauri, scripts, SSH config handling, settings persistence, mock/desktop backend boundaries, profile or skill sync flows, safety gates, or release readiness.
---

# CodexHub 项目维护指南

## 项目定位

CodexHub 是跨平台桌面控制面板，用于安全管理 Codex App 的 SSH 多服务器工作流。MVP 以 Tauri 2 + React + TypeScript + Vite + Rust 实现，通过本机 OpenSSH/SSH/SFTP 管理远端 Codex 配置和技能目录，而不是替代 Codex App。

当前进展：v0.2.2 已有桌面 UI、本地设置持久化、关闭按钮偏好与 Windows 托盘 / macOS 菜单栏状态图标、本地 SSH 状态检测、非覆盖式 Ed25519 key 生成、CodexHub 托管 SSH Host 块增删改查、远端 Codex 探测/安装/更新、profile/API config 管理、远端 config preview/apply、skill 本地库与安装目标管理、任务日志脱敏，`dev`/`stable` 发布通道，stable-only Tauri updater 基础，macOS release-build workflow，以及发布前总控验证脚本。

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

## 发布通道边界

- v0.2.0 起只保留 `dev` 和 `stable` 两个通道；不要新增 alpha、beta、nightly、staging、rc 或 preview 通道。
- `stable` 使用 `src-tauri/tauri.conf.json`，保持用户可见品牌 `CodexHub`，identifier 为 `app.codexhub.desktop`，窗口标题为 `CodexHub`；它只代表测试通过、无个人/本机信息泄漏且用户明确允许公开上线的版本。
- `dev` 使用 `src-tauri/tauri.dev.conf.json`，品牌为 `CodexHub Dev`，identifier 为 `dev.codexhub.desktop`，窗口标题为 `CodexHub Dev`；开发、测试、预览和人工验收都走 dev。
- 自动更新只允许 `stable` 使用 Tauri updater；Settings 的版本信息卡片放在本地密钥下方，检查按钮在 stable 正式包中可点击，feed/pubkey 未配置时必须返回 `pending-configuration` 而不是伪装可用，更新按钮只在检查返回 `available` 后启用；`CODEXHUB_STABLE_UPDATER_PUBKEY` 可是 Tauri updater 公钥行或生成的 minisign `.pub` 文件值，但构建/runtime 必须归一化为 Tauri 直接使用的公钥字符串；Windows signed updater 由 `.github/workflows/build-windows-release.yml`、`scripts/create-updater-tauri-config.mjs` 和 `scripts/create-windows-updater-feed.mjs` 产出 NSIS setup `.exe`、`latest.json` 和 `SHA256SUMS.txt`，独立 `.exe.sig` 不作为公开 Release 资产上传，临时 `src-tauri/tauri.updater.local.json` 不可提交，只有手动 dispatch 且 `upload_to_release=true` 才上传到 GitHub Release。
- 发布前总控使用 `scripts/validate-release.ps1`；`dev` 只做本地开发验收和源码预览，不生成公开 release artifact，`stable` 必须带 `-UserTested` 且完成 release build、public audit、启动检查和适用的 updater/feed 检查；v0.2.2 Windows 公开 Release 只保留可自动更新的 setup 安装包、`latest.json` 和 `SHA256SUMS.txt`，portable packaging 仅作为手工/本地包能力保留；macOS Apple Silicon 可补充 unsigned/ad-hoc `.dmg` 和 updater `.app.tar.gz`，unsigned/notarization 说明只写在 docs 和 GitHub Release notes，不写进软件 UI。
- macOS 构建使用 GitHub Actions 的 `Build macOS Release` workflow，在 macOS runner 上产出 `.app`/`.dmg` artifact；手动 dispatch 且 `upload_to_release=true` 时可上传 unsigned/ad-hoc Apple Silicon `.dmg`、`.app.tar.gz`、合并后的 `latest.json` 和 `SHA256SUMS.txt` 到既有 Release；未配置 Apple Developer ID 签名和 notarization 前必须在文档/Release 标记为 unsigned，并保留 `Requires real macOS test` 的验证边界。
- 不运行 live SSH acceptance，除非用户明确提供测试 alias；不 push、不打 tag、不创建 GitHub Release，除非用户另行明确要求。
- 本地 app 数据隔离依赖 Tauri `app_config_dir()` / `app_cache_dir()` 按 bundle identifier 分目录；不要改成手写 `%APPDATA%` 路径。
- 通道隔离不自动隔离 `%USERPROFILE%\.ssh\config`、本地 SSH key、远端 `~/.codex/config.toml`、远端 `~/.codex/skills/` 或远端 shell 文件；这些共享面仍必须遵守预览、备份、幂等和脱敏日志规则。
- README 面向普通用户；开发、测试、发布、通道和数据隔离细节写入 `docs/`，尤其是 `docs/release-channels.md` 和 `docs/release-checklist.md`。

## 代码约定

- 前端类型集中在 `src/models.ts`，Tauri 调用和 web/mock fallback 集中在 `src/api.ts`，设置归一化和本地 fallback 在 `src/settings.ts`。
- `safeInvoke` 用于可降级读取/mock 操作；真正会写入系统或需要桌面后端的操作用 `requiredInvoke`，让错误显式暴露给 UI。
- Rust 命令使用 `#[tauri::command]` 暴露，serde 字段保持 `camelCase` 与 TypeScript 类型一致；枚举命名要和前端 union 对齐。
- 桌面生命周期使用 Tauri 托盘/状态栏图标和 `closeButtonBehavior` 设置；窗口关闭按钮只负责 `ask` / `exit` / `minimize-to-tray` 偏好，托盘 Quit、macOS 应用菜单 Quit 和 `Cmd+Q` 必须保持真实退出。macOS 状态栏、隐藏/恢复和 Quit 行为在真实 Mac 验证前标记 `Requires real macOS test`。
- SSH config 逻辑放在 `src-tauri/src/ssh.rs`；解析、幂等更新、拒绝非托管冲突和 backup 行为必须有 Rust 单元测试。
- UI 文案使用 `src/App.tsx` 的 `uiCopy.en` / `uiCopy.zh` 双语结构；新增页面或按钮时同步两种语言。
- CSS 延续 `src/styles.css` 的变量体系和响应式断点；不要引入新的视觉系统，除非用户明确要求。

## 常用验证

优先按改动范围选择最小验证集：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-release.ps1 -Channel dev -SkipTauriBuild -SkipPortable -NoLive
pnpm smoke
pnpm smoke:mock
pnpm typecheck
pnpm build:web
cargo test --manifest-path src-tauri/Cargo.toml
git diff --check
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
