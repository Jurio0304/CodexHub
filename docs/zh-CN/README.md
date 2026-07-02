<div align="center">
  <img src="../../figs/app-logo.png" alt="CodexHub logo" width="104" height="104" />

  <h1>CodexHub</h1>

  <p><strong>面向 Codex App SSH 工作流的通用桌面控制台，支持 Windows 和 macOS。</strong></p>
  <p>准备 Linux 主机、安装或更新远端 Codex、应用 profile、同步 skills，并查看脱敏任务日志；不写入 Codex App 私有状态。</p>

  <p>
    <a href="../../README.md">English README</a>
    ·
    <a href="#-安装">安装</a>
    ·
    <a href="../known-limitations.md">已知限制</a>
    ·
    <a href="../../SECURITY.md">安全策略</a>
  </p>

  <p>
    <img alt="Release" src="https://img.shields.io/badge/release-v0.2.0-2563eb" />
    <img alt="License" src="https://img.shields.io/badge/license-MIT-16a34a" />
    <img alt="Platform" src="https://img.shields.io/badge/platform-Windows%20%2B%20macOS-0078D4" />
    <img alt="Tauri" src="https://img.shields.io/badge/Tauri-2-24C8DB" />
    <img alt="React" src="https://img.shields.io/badge/React-18-61DAFB" />
    <img alt="Rust" src="https://img.shields.io/badge/Rust-stable-B7410E" />
  </p>
</div>

## 🧭 快速了解

CodexHub 聚焦一个清晰场景：让 Windows 或 macOS 桌面上的 Codex App 更安全、可审计地使用多台 SSH Linux 主机。

- 管理本地 OpenSSH key 状态和 CodexHub 托管的 SSH alias。
- 用一次性密码初始化新 Linux 主机，再切换到 key 登录。
- 在修改前探测远端 Codex、config、shell、PATH 和 skill 状态。
- 预览并应用 Codex profile 和 skills，保留备份与脱敏日志。
- 验证 SSH alias 后，引导用户去 Codex App `Settings > Codex > Connections` 添加连接。

## 🖼️ 截图

【截图占位：Dashboard 主机矩阵，展示 SSH、Codex、Profile、Skill、Task 状态。】

【截图占位：Add Server 一次性密码引导、公钥安装和托管 SSH config 写入流程。】

【截图占位：Profiles 页面，展示预览、应用配置和任务日志。】

【截图占位：Skills 页面，展示本地技能库、目标检测和安装/卸载操作。】

## ✨ 核心能力

- 检测 Windows 和 macOS 的本地 OpenSSH、本地公钥和 SSH config 状态。
- 在没有合适密钥时生成不覆盖旧文件的 Ed25519 key。
- 只读导入本地 SSH config 中安全的 Host alias（Windows 为 `%USERPROFILE%\.ssh\config`，macOS 为 `~/.ssh/config`）。
- 只写入 CodexHub 托管的 SSH config block，并在写入前备份。
- 通过 `ssh <HostAlias> echo ok` 测试连接。
- 探测远端 Linux 主机的系统、架构、shell、PATH、Codex CLI、`~/.codex/config.toml` 和 skills 数量。
- 在远端用户目录安装或更新真实的 `codex` 命令，不安装 wrapper。
- 创建、预览、应用 profile 到远端 `~/.codex/config.toml`。
- 导入本地或 GitHub skill，并安装到本机或远端目标。
- 在 Tasks 中查看命令、stdout/stderr、退出码、耗时和失败原因，日志默认脱敏。
- 完成准备后，引导用户到 Codex App 手动添加或启用已验证的 SSH alias。

## 🔐 安全边界

- 不读取、不显示、不保存 SSH 私钥、passphrase、一次性密码或 OpenAI API key 明文。
- UI 只返回和复制 public key。
- 不修改非 CodexHub 托管的 SSH config 内容。
- 托管 Host block 使用 `# >>> CodexHub managed host: <alias>` 和 `# <<< CodexHub managed host: <alias>` 标记。
- 不写 Codex App 私有文件、数据库、socket、缓存或未公开 IPC。
- 远端 Codex 配置使用 `env_key` / `apiKeyEnvVar` 引用远端环境变量。
- 本地 credential store 中的 API key 不会写入远端 config、metadata 或 task log。

更多说明见：[安全策略](../../SECURITY.md)、[已知限制](../known-limitations.md)。

## ✅ 运行要求

Windows 桌面应用需要：

1. Windows 10/11。
2. Microsoft WebView2 Runtime。
3. Windows OpenSSH client：`ssh.exe`、`scp.exe`、`ssh-keygen.exe`。
4. 可通过 SSH 登录的 Linux 远端主机。

macOS 桌面应用需要：

1. 一台真实 Mac 用于 `.app` / `.dmg` 运行验证。
2. OpenSSH client tools 和 `ssh-keygen`。
3. 通过 OpenAI/Codex 官方指引安装 Codex CLI。
4. 可通过 SSH 登录的 Linux 远端主机。

## 🚀 安装

日常使用建议从本仓库的 Releases 页面下载最新 stable 构建。

- Windows 安装包：下载并运行 Windows x64 setup `.exe`。
- Windows 便携包：解压 Windows x64 portable `.zip`，然后运行 `CodexHub.exe`。
- macOS：下载 macOS Apple Silicon `.dmg`；当前 artifact 未签名，可能需要通过 Gatekeeper 手动允许。

## ⚡ 快速开始

1. 打开 CodexHub。
2. 在 Settings 检查 Local SSH。
3. 没有 key 时生成 Ed25519 key；已有 key 时不要覆盖。
4. 添加 SSH host，填写 host、user、port 和 identity file。
5. 对尚未配置公钥登录的远端，使用一次性密码引导。
6. 测试 SSH alias，并探测远端主机。
7. 安装或更新远端 Codex CLI。
8. 创建 profile，先 preview，再 apply。
9. 导入 skill，并安装到本机或远端。
10. 打开 Tasks 查看脱敏日志。
11. 到 Codex App `Settings > Codex > Connections` 添加或启用该 SSH alias。

## 📘 使用流程

### 添加主机

- 使用 Hosts > Add Server 创建新的 CodexHub 托管 alias。
- 现有 alias 可以从本地 SSH config 只读导入，不会重写非托管 block。
- 新托管主机只有在密码登录、公钥安装、权限修复和 key 登录验证成功后才写入。
- 首次 host key 使用 OpenSSH `StrictHostKeyChecking=accept-new`；host key 改变时仍会失败。

### 安装或更新 Codex

- 通过 Profiles 或 Dashboard 操作执行 `check-version`、`install` 或 `update`。
- 远端命令保持为 `codex`；CodexHub 不安装 wrapper。
- 安装目标为 `$HOME/.local/bin` 和 `$HOME/.codex`。
- PATH 修复是 `.bashrc` 或 `.zshrc` 中幂等的 CodexHub 托管 block。
- 优先尝试官方 installer；mirror 和本地上传 fallback 会记录到日志。

### 应用 Profile

- Profiles 渲染为 TOML。
- API key 使用环境变量引用。
- 应用前先预览。
- 如果远端 config 已一致，CodexHub 报告 no changes，不创建备份。
- 如果文件发生变化，CodexHub 创建时间戳备份，并在 Tasks 中记录结果。

### 安装 Skills

- 可以导入包含 `SKILL.md` 的本地目录，也可以导入 GitHub 仓库/子目录 URL。
- CodexHub 会在 app config 目录保存一份托管本地副本。
- 目标检测使用缓存 inventory；给新主机安装前请先运行检测。
- 卸载会把本地和远端 skill 目录移动到备份，而不是硬删除。
