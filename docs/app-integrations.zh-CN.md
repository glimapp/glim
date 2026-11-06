# 应用集成

[English](app-integrations.md)

进入 **设置 → 应用集成**，在检测到的应用旁点击 **关联**。绿色点与“已关联”表示配置已写入，不表示目标应用已经建立实时连接。重启对应应用或重新加载其 MCP 服务，并按目标应用要求批准工具调用。“已关联”菜单提供取消关联。

| 应用 | 用户级配置位置 | 使用方式 |
| --- | --- | --- |
| Codex | `~/.codex/config.toml`、检测到的 `~/.codex-*` 目录、`CODEX_HOME` 或手动选择的目录 | 添加本地与 SSH 远端路径的 Glim 打开入口，并设为默认。 |
| Claude Desktop | `~/Library/Application Support/Claude/claude_desktop_config.json` | 在对话中要求用 Glim 打开本地文件或文件夹。 |
| Claude Code | `~/.claude.json` 或 `$CLAUDE_CONFIG_DIR/.claude.json` | 同上，使用本地工具。 |
| Cursor | `~/.cursor/mcp.json` | 同上。 |
| Gemini CLI | `~/.gemini/settings.json` | 同上。 |
| GitHub Copilot | `$COPILOT_HOME/mcp-config.json` 或 `~/.copilot/mcp-config.json` | 供 Copilot CLI 与 VS Code 的 Copilot 模式使用。 |
| GitHub Copilot · VS Code | `~/Library/Application Support/Code/User/mcp.json` | 供 VS Code 的 Local 模式及兼容的旧版使用。 |
| OpenCode | `$OPENCODE_CONFIG`，或 `~/.config/opencode/opencode.jsonc` / `opencode.json` | 使用同一本地工具，写入 OpenCode 的 `mcp` 对象和本地命令数组。 |

**添加配置…** 先选择已支持的工具，再选择配置文件或目录。选择后加入列表，点击“关联”才会修改对应应用的配置。

关联 Codex 时，同一次操作会将 `desktop.open-in-target-preferences.global` 以及已有的 `perPath` 项目偏好设为 `custom:glim`，因此已保存过编辑器选择的项目也会使用 Glim。修改仅作用于所选实例，不改变 macOS 的文件关联。界面不提供单独的“设为默认”操作，也不维护独立的默认状态。实例发现只读取配置，保留用户之后在 Codex 中改选的编辑器。取消关联只清理仍指向 Glim 的偏好，由 Codex 选择回退工具。外部修改配置后需要重启 Codex；“已关联”只确认配置已写入，不能证明运行中的客户端已经加载菜单。

打开设置、返回设置所在窗口或点击刷新时都会重新发现实例。扫描范围包括主目录、`~/.config` 和 `~/Library/Application Support` 下识别为对应工具的目录，支持带后缀的配置目录、账号管理器目录及 VS Code Profiles，每个已识别目录最多向下查找三层，跳过会话、缓存、项目与插件数据。按工具类型和配置文件的真实路径去重；共用一个配置文件的多个窗口或应用副本只展示一项。同一工具有多个配置时，在名称下方显示目录，方便区分。

应用识别会读取 `/Applications`、`~/Applications` 及其下一层分类目录中的 bundle ID，支持改名安装；同时检查当前进程环境、launchd 环境和应用的 `LSEnvironment`，识别 `CODEX_HOME`、`CLAUDE_CONFIG_DIR`、`COPILOT_HOME`、`OPENCODE_CONFIG`、`OPENCODE_CONFIG_DIR`、`XDG_CONFIG_HOME` 等配置覆盖。不会执行启动脚本、读取进程凭据或扫描整个磁盘；藏在任意脚本或非常规位置的配置仍可手动添加。

Codex 使用 `glim --open-from-codex`，配置为 `input = "json_stdin"` 与 `supports_ssh = true`。SSH 请求携带连接信息、远端工作区和文件路径；Glim 复用匹配的 SSH 连接与工作区，再通过现有 SFTP 阅读流程打开文件。主机别名沿用本机 OpenSSH 配置，连接失败复用工作区原有错误展示。Glim 不执行 Codex 提供的终端命令，也不复制私钥。启动时会升级识别为 Glim 的旧版本地关联，沿用备份与冲突检查，保留全部默认编辑器偏好；每个受影响的 Codex 实例需要重启以重新加载处理器。

项目级设置、组织策略及客户端版本差异可能覆盖用户级配置。纯网页工具与非 SSH 云端环境暂不支持，其他客户端的 MCP 集成仅接收本地路径。

MCP 命令使用已安装的 `Glim.app/Contents/MacOS/glim --mcp`，无需额外安装 Node.js、Python、网络监听器或常驻服务。`open_in_glim` 接收一个本地绝对路径。调用成功表示已交给 macOS 打开，不代表文件渲染完成；工具不上传或返回文档内容。

现有配置的备份位于 `~/Library/Application Support/Glim/Integrations/Backups`，受私有目录权限保护，每个文件最多保留两份。取消关联删除识别为 Glim 的条目及 Codex 中指向 Glim 的默认项，保留其他集成与设置。已有配置格式错误或存在同名冲突时不覆盖；可点击对应行的 Finder 按钮检查，再刷新状态。

各客户端的官方配置文档见[英文版参考链接](app-integrations.md#client-references)。
