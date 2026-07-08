---
name: codexhub-maintainer
description: Use this project skill when developing, reviewing, testing, or maintaining CodexHub, a cross-platform Tauri 2 + React + TypeScript + Rust desktop control plane for Codex App SSH multi-server workflows. Trigger for changes touching README/docs, src, src-tauri, scripts, SSH config handling, settings persistence, mock/desktop backend boundaries, profile or skill sync flows, safety gates, or release readiness.
---

# CodexHub 项目维护指南

## 项目定位

CodexHub 是跨平台桌面控制面板，用于安全管理 Codex App 的 SSH 多服务器工作流。MVP 以 Tauri 2 + React + TypeScript + Vite + Rust 实现，通过本机 OpenSSH/SSH/SFTP 管理远端 Codex 配置和技能目录，而不是替代 Codex App。

当前进展：v0.4.2 已有桌面 UI、本地设置持久化、关闭按钮偏好与 Windows 托盘 / macOS 菜单栏 / Linux 托盘状态图标、本地 SSH 状态检测、非覆盖式 Ed25519 key 生成、CodexHub 托管 SSH Host 块增删改查、远端 Codex 探测/安装/更新与 command/env 就绪校验、profile/API config 管理、远端 config preview/apply、skill 本地库与安装目标管理、任务日志脱敏，`dev`/`stable` 发布通道，stable-only Tauri updater 基础，检查更新任务日志与失败弹窗，侧边栏进程完成视觉提示，所有已记住主机的只读资源监控页，macOS release-build workflow、Ubuntu/Debian amd64 + arm64 Linux deb release workflow、v0.4.2 macOS 资产已完成真实 Mac 验证，以及发布前总控验证脚本。

## 开发优先级

1. 先保护用户环境，再实现功能：任何写入 SSH config、远端 `~/.codex/config.toml`、远端 `~/.codex/skills/` 的路径都必须可预览、可备份、可重复执行、可恢复。
2. 继续沿用直接 SSH/SFTP 管理路线：MVP 不依赖独立的远端 wrapper 命令，远端用户可见命令保持为 `codex`，不写 Codex App 私有状态，不假设存在未公开的 host 注册或 reconnect API。
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
- 已安装 skill 标签预览弹窗中的“卸载”是用户二次确认后的直删语义：只删除当前本机/主机目标上的该 skill 目录，弹窗必须提示无法恢复，并用任务日志记录结果。
- 已安装 skill 标签预览要使用已扫描 inventory 中的 `SKILL.md` description；远端 skill 未下载到本地库时只能只读展示说明，不显示编辑入口。下载/卸载二次确认后必须立即进入可隐藏的日志弹窗，后台继续执行，避免确认弹窗 busy 遮住整个应用。
- 操作日志和错误信息默认去除 key material、token、passphrase；必要时再考虑用户名/主机名脱敏。
- 远端 Codex 就绪状态必须区分 `~/.local/bin/codex` 文件存在、当前 shell / 登录 shell 可直接 `command -v codex`、以及 `env_key` 指向的远端环境变量是否存在；只检查环境变量存在性，不打印值。
- 显式应用带有本地 credential-store key 的 profile 时，允许只把真实 key 写入选中远端的 `~/.codex-hub/env`（目录 `700`、文件 `600`、替换前备份、日志脱敏），并可安装同名 `~/.local/bin/codex` 托管 launcher 来 source 该 env 后 exec 真实 Codex；禁止把 key 写入远端 `~/.codex/config.toml`、`applied-profile.json`、app JSON 或 task log。
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
- 普通 `master` push / pull_request 只运行 `.github/workflows/ci.yml` 的轻量源代码验证；Windows、macOS、Linux release workflows 必须保持 `workflow_dispatch` 手动触发，不要接回 `push` 或 `pull_request`。
- 自动更新只允许 `stable` 使用 Tauri updater；Settings 的版本信息卡片放在本地密钥下方，检查按钮在 stable 正式包中可点击，feed/pubkey 未配置时必须返回 `pending-configuration` 而不是伪装可用，更新按钮只在检查返回 `available` 后启用；检查更新每次记录 `Check app update` 任务，失败时弹出日志窗口并提示用户可在 Tasks 详情页回看，不在版本卡片下方直接展开完整错误；`CODEXHUB_STABLE_UPDATER_PUBKEY` 首选 `tauri signer generate` 生成的 Tauri `.key.pub` 值，也可兼容 raw minisign `.pub` 文本或 bare key 行，但构建/runtime 必须归一化为 Tauri 期望的 base64 pub-file 值；Windows signed updater 由 `.github/workflows/build-windows-release.yml`、`scripts/create-updater-tauri-config.mjs` 和 `scripts/create-windows-updater-feed.mjs` 产出 NSIS setup `.exe`、`latest.json` 和 `SHA256SUMS.txt`，独立 `.exe.sig` 不作为公开 Release 资产上传，临时 `src-tauri/tauri.updater.local.json` 不可提交，只有手动 dispatch 且 `upload_to_release=true` 才上传到 GitHub Release。
- 发布前总控使用 `scripts/validate-release.ps1`；`dev` 只做本地开发验收和源码预览，不生成公开 release artifact，`stable` 必须带 `-UserTested` 且完成 release build、public audit、启动检查和适用的 updater/feed 检查；v0.2.5 Windows 公开 Release 只保留可自动更新的 setup 安装包、`latest.json` 和 `SHA256SUMS.txt`，portable packaging 仅作为手工/本地包能力保留；macOS Apple Silicon 可补充 unsigned/ad-hoc `.dmg` 和 updater `.app.tar.gz`；Ubuntu/Debian Linux 目前只发布 amd64 与 arm64 `.deb` 手动安装包，暂不发布 AppImage，暂不向 `latest.json` 添加 Linux 平台键；unsigned/notarization 或 Linux 发行格式边界说明只写在 docs 和 GitHub Release notes，不写进软件 UI。
- 公开 GitHub Release notes 必须使用英文 + 简体中文双语；Highlights、Assets、updater feed 说明，以及 macOS unsigned/ad-hoc 和签名/notarization 边界都要两种语言覆盖，禁止只发布英文 Release notes；后续不要再加入“需要在真实 Mac 上验证后再大范围分发”这类提示。
- macOS 构建使用 GitHub Actions 的 `Build macOS Release` workflow，在 macOS runner 上产出 `.app`/`.dmg` artifact；手动 dispatch 且 `upload_to_release=true` 时可上传 unsigned/ad-hoc Apple Silicon `.dmg`、`.app.tar.gz`、合并后的 `latest.json` 和 `SHA256SUMS.txt` 到既有 Release；未配置 Apple Developer ID 签名和 notarization 前必须在文档/Release 标记为 unsigned，并在 macOS 行为或打包改变后重新执行真实设备验证。
- 不运行 live SSH acceptance，除非用户明确提供测试 alias；不 push、不打 tag、不创建 GitHub Release，除非用户另行明确要求。
- 本地 app 数据隔离依赖 Tauri `app_config_dir()` / `app_cache_dir()` 按 bundle identifier 分目录；不要改成手写 `%APPDATA%` 路径。
- 通道隔离不自动隔离 `%USERPROFILE%\.ssh\config`、本地 SSH key、远端 `~/.codex/config.toml`、远端 `~/.codex/skills/` 或远端 shell 文件；这些共享面仍必须遵守预览、备份、幂等和脱敏日志规则。
- README 面向普通用户；开发、测试、发布、通道和数据隔离细节写入 `docs/`，尤其是 `docs/release-channels.md` 和 `docs/release-checklist.md`。

## 代码约定

- 前端类型集中在 `src/models.ts`，Tauri 调用和 web/mock fallback 集中在 `src/api.ts`，设置归一化和本地 fallback 在 `src/settings.ts`。
- `safeInvoke` 用于可降级读取/mock 操作；真正会写入系统或需要桌面后端的操作用 `requiredInvoke`，让错误显式暴露给 UI。
- Rust 命令使用 `#[tauri::command]` 暴露，serde 字段保持 `camelCase` 与 TypeScript 类型一致；枚举命名要和前端 union 对齐。
- 桌面生命周期使用 Tauri 托盘/状态栏图标和 `closeButtonBehavior` 设置；窗口关闭按钮只负责 `ask` / `exit` / `minimize-to-tray` 偏好，托盘 Quit、macOS 应用菜单 Quit 和 `Cmd+Q` 必须保持真实退出。macOS 状态栏、隐藏/恢复、Quit 行为和 v0.4.2 macOS 公开资产已有真实 Mac 验证基础；后续生命周期或打包改动需要重新验证。
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

如果本机 `node` 或 `pnpm` 不可用，或出现 `pnpm` 命令本身可执行但内部报 `node` not found / 无法识别 `node`，先判定为本机 PATH 不完整，不要反复更换验证命令或怀疑功能代码。优先使用 Codex 桌面提供的 bundled Node/pnpm，并显式把 Node 与 pnpm 目录都前置到 PATH：

```powershell
$codexNodeDir = "$env:USERPROFILE\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin"
$codexPnpmDir = "$env:USERPROFILE\.cache\codex-runtimes\codex-primary-runtime\dependencies\bin"
$env:PATH = "$codexNodeDir;$codexPnpmDir;$env:PATH"
& "$codexPnpmDir\pnpm.cmd" typecheck
& "$codexPnpmDir\pnpm.cmd" smoke
& "$codexPnpmDir\pnpm.cmd" dev
```

完整 Tauri 桌面验证需要 Node 20+、pnpm、Rust stable MSVC、WebView2 和 Windows OpenSSH。

不要打开 Codex App 内置浏览器做视觉或运行时检查：当前内置浏览器存在打开后导致 Codex App 闪退的已知问题。优先使用静态断言、CLI 验证、用户提供截图，必要时先征得用户同意再用外部浏览器或无头 Playwright。

## 维护流程

1. 阅读 `README.md`、`docs/architecture.md`、`docs/mvp-scope.md`、`docs/known-limitations.md` 中与任务相关的部分。
2. 运行或检查 `git status --short --branch`，不要覆盖用户已有改动。
3. 确认当前工作位于临时开发分支；如需从主分支切出，先保证工作区干净或得到用户明确指示。
4. 做最小实现；涉及文件写入时先补安全门禁和测试，再接 UI。
5. 更新 README 或 docs 时只写当前事实：区分 mock、已连真实桌面后端、尚未实现的远端能力。
6. 结束前说明实际验证命令、结果、当前分支和是否等待用户确认合并；若只验证了 web/mock，不要声称完整桌面或远端流程已通过。
