# CodexHub：把 Codex App 的 SSH 多服务器工作流装进一个桌面控制台

> 文章标签：Codex App、SSH、Tauri、React、Rust、AI 开发工具、远程开发
> 分类专栏：AI 工具链 / 桌面应用 / 远程开发效率
> 项目地址：<CodexHub 仓库地址>
> Release 下载：<CodexHub Release 页面>

如果你已经开始用 Codex App 做日常开发，大概率会遇到一个很现实的问题：本地电脑只是入口，真正的项目、依赖、GPU 环境、长期运行服务，往往分散在多台 Linux 服务器上。

这时候，SSH 连接、远端 Codex CLI、`~/.codex/config.toml`、skills 同步、profile 切换、日志排查，就会慢慢变成一堆散落在终端、文档和脑子里的操作步骤。

CodexHub 想解决的正是这件事：它不是要替代 Codex App，而是做一个面向 Codex App SSH 工作流的桌面控制台，帮你把多台远端主机的准备、检查、配置和同步流程变得可视、可预览、可追踪。

> 【截图占位 01：文章封面图】
> 建议放置 CodexHub logo + Dashboard 总览页拼图，突出“Codex App SSH 多服务器控制台”的第一印象。

## 为什么需要 CodexHub

当你只连接一台服务器时，手动维护 SSH 配置还可以接受。但当远端环境变多之后，很多事情会变得重复而容易出错：

- 本地有没有可用的 OpenSSH 和 Ed25519 公钥？
- 某台服务器的 SSH alias 是否能正常连通？
- 远端是否已经安装 Codex CLI？版本是否需要更新？
- `~/.local/bin` 是否在 PATH 里？
- 远端 `~/.codex/config.toml` 当前是什么状态？
- 本地 profile 应用到远端前，变更内容能不能先预览？
- skills 装到了本机还是远端？哪些主机已经同步？
- 上一次失败到底是网络、权限、PATH，还是远端依赖问题？

CodexHub 把这些分散步骤收进一个桌面应用里，让你在真正打开 Codex App 进入开发前，先把远端工作空间准备好。

## 一屏看清所有主机状态

CodexHub 的 Dashboard 用来查看所有托管主机的整体状态，包括 SSH 连通性、远端 Codex 状态、profile 对齐情况、skill inventory 和近期任务结果。

你可以把它理解成一个面向 Codex SSH 工作流的“控制面板”：先看哪里没准备好，再点进对应页面处理。

> 【截图占位 02：Dashboard 总览页】
> 建议截图内容：主机列表、SSH 状态、Codex 状态、profile/skills 状态、近期任务结果。

## 添加主机：从一次性密码到密钥登录

在 Hosts 页面，你可以添加或检查 SSH 主机。CodexHub 支持读取本地 SSH config 中安全的 Host alias，也支持创建 CodexHub 托管的 Host block。

对于新服务器，它可以使用一次性密码完成初始化：安装本地 public key、修复远端 `~/.ssh` 权限、写入受控 SSH 配置，然后验证 key 登录。这个密码只用于当前引导流程，不会被保存。

更重要的是，CodexHub 不会整体覆盖你的 SSH config。它只管理带有 CodexHub 标记的配置块，并在写入前创建时间戳备份。

> 【截图占位 03：Hosts 主机管理页】
> 建议截图内容：Add Server 表单、SSH test、remote probe、一次性密码初始化流程。

## 安装和更新远端 Codex：保持真实的 `codex` 命令

CodexHub 的远端维护路线很克制：它通过 SSH/SFTP 管理 Linux 主机，不在远端安装强绑定 wrapper，也不改 Codex App 私有状态。

安装或更新 Codex 时，CodexHub 会在远端用户目录下准备环境，保持最终命令仍然是正常的 `codex`。如果 `~/.local/bin` 不在 PATH 中，它会以可重复执行的方式修复 shell 配置，并记录完整任务日志。

这意味着你后续在 Codex App 或普通 SSH 会话里使用的仍然是标准 Codex CLI，而不是某个不可见的中间层。

## Profiles：远端配置先预览，再应用

很多人使用远端 Codex 时，真正担心的不是“能不能写配置”，而是“写之前我能不能确认它会改什么”。

CodexHub 的 Profiles 页面支持管理本地 profile 模板，渲染为 TOML 后再应用到远端 `~/.codex/config.toml`。应用前可以 preview，真正写入时会备份原文件；如果内容完全一致，则报告 no changes，不制造多余备份。

API key 处理也采用更安全的方式：远端配置使用环境变量引用，而不是把明文 key 写进远端 config、metadata 或任务日志。

> 【截图占位 04：API 与 Profiles 页面】
> 建议截图内容：profile 模板列表、远端配置 preview、apply 结果、API env var 设置。

## Skills：本地导入、GitHub 导入、远端同步

CodexHub 也把 skills 管理纳入了同一个工作流。你可以导入包含 `SKILL.md` 的本地目录，也可以导入 GitHub 仓库或仓库子目录 URL。

导入后，CodexHub 会在自己的 app config 目录保存一份托管副本。安装到本机或远端目标时，会基于 inventory 判断哪些目标可安装、哪些已经存在。卸载时也不是硬删除，而是移动到备份目录，方便回滚。

> 【截图占位 05：Skills 技能库页面】
> 建议截图内容：本地 skill library、installed skill inventory、安装目标选择、任务结果。

## Tasks：每次操作都有脱敏日志

远程环境出问题时，最怕的是“按钮点了，但不知道它到底做了什么”。CodexHub 的 Tasks 页面会记录命令状态、耗时、stdout/stderr、失败原因和关键证据，并默认脱敏 key material、token、passphrase 等敏感信息。

对于 SSH、安装、更新、profile apply、skill install 这类操作，任务日志能让你快速判断问题发生在哪一步。

> 【截图占位 06：Tasks 任务日志页】
> 建议截图内容：一次成功任务和一次失败任务的日志详情，注意发布前遮挡真实主机名、用户名和路径。

## Settings：本地 SSH、更新检查和桌面偏好

Settings 页面负责本地 SSH 就绪检查、公钥复制、应用更新检查，以及窗口关闭行为等桌面偏好。

Windows 版本支持托盘图标；macOS 版本面向 Apple Silicon 提供 `.dmg` 安装包，v0.4.3 本轮仍需要真实 Mac 验证；当前 macOS 构建仍是 unsigned/ad-hoc，首次打开可能需要通过系统安全设置手动允许。日常使用请优先从 GitHub Releases 下载可信构建。

> 【截图占位 07：Settings 设置页】
> 建议截图内容：Local SSH 检查、公钥状态、版本信息、更新检查、关闭按钮偏好。

## 它适合谁

CodexHub 特别适合这些场景：

- 你在 Windows 或 macOS 上使用 Codex App。
- 你经常连接多台 Linux 远程开发机。
- 你希望远端 Codex CLI、profile、skills 有统一的检查和同步入口。
- 你不想靠零散命令手动维护 SSH config 和远端 `~/.codex`。
- 你在意变更前预览、写入前备份，以及失败后可追踪日志。

如果你要的是完整终端模拟器、团队级服务器平台、RBAC、无人值守批量运维，CodexHub 当前并不定位在这些方向。它更像一个轻量但可靠的个人桌面控制台，专注把 Codex App 的 SSH 远程工作流打磨顺。

## 技术栈

CodexHub 使用 Tauri 2 + React + TypeScript + Vite + Rust 构建。

前端负责桌面交互和状态呈现，Rust 后端负责本地 OpenSSH 调用、SSH config 解析、远端探测、文件备份、SFTP/SSH 操作和脱敏任务日志。这样的组合让它既保持桌面应用的轻量体验，也能安全地接触本地和远端系统能力。

它遵循一个很明确的边界：CodexHub 管理 SSH 和远端 Codex 配置，但不写 Codex App 私有数据库、socket、缓存或未公开状态。完成准备后，它会引导你到 Codex App 的 `Settings > Codex > Connections` 手动添加或启用已验证的 SSH alias。

## 快速开始

1. 前往 GitHub Releases 下载最新 stable 版本。
2. 打开 CodexHub，在 Settings 检查本地 SSH 状态。
3. 如果没有合适的 key，再生成不覆盖旧文件的 Ed25519 key。
4. 添加 SSH host，填写 host、user、port 和 identity file。
5. 对新服务器使用一次性密码引导，再切换到 key 登录。
6. 测试 SSH alias，并探测远端 Codex 状态。
7. 安装或更新远端 Codex CLI。
8. 创建 profile，先 preview，再 apply 到远端。
9. 导入 skill，并安装到本机或远端目标。
10. 打开 Tasks 查看脱敏任务日志。
11. 回到 Codex App，在 `Settings > Codex > Connections` 中添加或启用该 SSH alias。

## 下载与反馈

项目地址：<CodexHub 仓库地址>

Release 下载：<CodexHub Release 页面>

如果你也在使用 Codex App 连接多台远程 Linux 主机，欢迎试试 CodexHub。它不追求把所有事情都自动化，而是先把最容易出错、最需要审计的远程准备流程做清楚：能看见、能预览、有备份、可回滚、日志可追踪。

也欢迎在 GitHub 上提交 issue、建议和使用反馈。如果这个工具刚好解决了你的远程 Codex 工作流痛点，可以点一个 Star 支持一下。
