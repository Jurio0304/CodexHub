# CodexHub 中文说明

CodexHub 是一个 Windows 优先的桌面控制台，用来安全管理 Codex App 的 SSH 多服务器工作流。它不替代 Codex App，也不写入 Codex App 私有状态；它只通过公开、可审计的 SSH/SCP/SFTP 路径帮助你准备远端 Linux 主机、安装或更新远端 Codex CLI、应用配置 profile、安装 skills，并查看脱敏任务日志。

英文主文档见：[README.md](../../README.md)

## 截图

【截图占位：Dashboard 主机矩阵，展示 SSH、Codex、Profile、Skill、Task 状态。】

【截图占位：Add Server 一次性密码引导、公钥安装和托管 SSH config 写入流程。】

【截图占位：Profiles 页面，展示预览、应用配置和任务日志。】

【截图占位：Skills 页面，展示本地技能库、目标检测和安装/卸载操作。】

## 核心能力

- 检测 Windows OpenSSH、本地公钥和 SSH config 状态。
- 在没有合适密钥时生成不覆盖旧文件的 Ed25519 key。
- 只读导入 `%USERPROFILE%\.ssh\config` 中安全的 Host alias。
- 只写入 CodexHub 托管的 SSH config block，并在写入前备份。
- 通过 `ssh <HostAlias> echo ok` 测试连接。
- 探测远端 Linux 主机的系统、架构、shell、PATH、Codex CLI、`~/.codex/config.toml` 和 skills 数量。
- 在远端用户目录安装或更新真实的 `codex` 命令，不安装 wrapper。
- 创建、预览、应用 profile 到远端 `~/.codex/config.toml`。
- 导入本地或 GitHub skill，并安装到本机或远端目标。
- 在 Tasks 中查看命令、stdout/stderr、退出码、耗时和失败原因，日志默认脱敏。
- 在完成准备后，引导用户到 Codex App `Settings > Codex > Connections` 手动添加或启用已验证的 SSH alias。

## 安全边界

- 不读取、不显示、不保存 SSH 私钥、passphrase、一次性密码或 OpenAI API key 明文。
- UI 只返回和复制 public key。
- 不修改非 CodexHub 托管的 SSH config 内容。
- 托管 Host block 使用 `# >>> CodexHub managed host: <alias>` 和 `# <<< CodexHub managed host: <alias>` 标记。
- 不写 Codex App 私有文件、数据库、socket、缓存或未公开 IPC。
- 远端 Codex 配置使用 `env_key` / `apiKeyEnvVar` 引用远端环境变量。
- 本地 credential store 中的 API key 不会写入远端 config、metadata 或 task log。

更多说明见：[安全策略](../../SECURITY.md)、[已知限制](../known-limitations.md)。

## 运行要求

完整桌面开发需要：

1. Windows 10/11。
2. Microsoft WebView2 Runtime。
3. Windows OpenSSH client：`ssh.exe`、`scp.exe`、`ssh-keygen.exe`。
4. Node.js 20+ 和 pnpm。
5. Rust stable MSVC toolchain。
6. 可通过 SSH 登录的 Linux 远端主机。

Mock mode 只需要 Node.js。

## 安装与运行

```powershell
pnpm install
pnpm dev
```

Web-only UI：

```powershell
pnpm dev:web
```

Mock mode：

```powershell
pnpm dev:mock
```

## 使用教程

1. 打开 CodexHub。
2. 在 Settings 检查 Local SSH。
3. 没有 key 时生成 Ed25519 key；已有 key 时不覆盖。
4. 添加 SSH host，填写 host、user、port 和 identity file。
5. 对尚未配置公钥登录的远端，使用一次性密码引导。
6. 确认 CodexHub 只写入托管 SSH config block。
7. 测试连接。
8. 探测远端系统和 Codex 状态。
9. 安装或更新远端 Codex CLI。
10. 创建 profile，先 preview，再 apply。
11. 导入 skill，并安装到本机或远端。
12. 打开 Tasks 查看脱敏日志。
13. 到 Codex App `Settings > Codex > Connections` 添加或启用该 SSH alias。

## 开发命令

```powershell
pnpm smoke
pnpm smoke:mock
pnpm typecheck
cargo test --manifest-path src-tauri/Cargo.toml
pnpm build:web
pnpm build:tauri
```

如果系统 PATH 没有 `node`，先把 Codex bundled Node/pnpm 路径放到 PATH 前面。
