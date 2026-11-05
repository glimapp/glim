# 应用集成

[English](app-integrations.md)

进入 **设置 → 应用集成**，在检测到的应用旁点击 **关联**。绿色点与“已关联”表示配置已写入，不表示目标应用已经建立实时连接。重启对应应用或重新加载其 MCP 服务，并按目标应用要求批准工具调用。“已关联”菜单提供取消关联。

| 应用 | 用户级配置位置 | 使用方式 |
| --- | --- | --- |
| Codex | `~/.codex/config.toml`、检测到的 `~/.codex-*` 目录、`CODEX_HOME` 或手动选择的目录 | 在“打开方式”中选择 Glim。 |
| Claude Desktop | `~/Library/Application Support/Claude/claude_desktop_config.json` | 在对话中要求用 Glim 打开本地文件或文件夹。 |
| Claude Code | `~/.claude.json` | 同上，使用本地工具。 |
| Cursor | `~/.cursor/mcp.json` | 同上。 |
| Gemini CLI | `~/.gemini/settings.json` | 同上。 |
| GitHub Copilot | `$COPILOT_HOME/mcp-config.json` 或 `~/.copilot/mcp-config.json` | 供 Copilot CLI 与 VS Code 的 Copilot 模式使用。 |
| GitHub Copilot · VS Code | `~/Library/Application Support/Code/User/mcp.json` | 供 VS Code 的 Local 模式及兼容的旧版使用。 |
| OpenCode | `$OPENCODE_CONFIG`，或 `~/.config/opencode/opencode.jsonc` / `opencode.json` | 使用同一本地工具，写入 OpenCode 的 `mcp` 对象和本地命令数组。 |

OpenCode 的目录发现遵循 `XDG_CONFIG_HOME`。**添加 Codex 配置…** 用于选择包含 `config.toml` 的其他目录，选中后不会立即修改配置。应用检测仅查看常见应用和配置位置，不扫描整个磁盘。项目级设置、组织策略及客户端版本差异可能覆盖用户级配置。纯网页工具与远端、云端文件不适用这条本地通道。

MCP 命令使用已安装的 `Glim.app/Contents/MacOS/glim --mcp`，无需额外安装 Node.js、Python、网络监听器或常驻服务。`open_in_glim` 接收一个本地绝对路径。调用成功表示已交给 macOS 打开，不代表文件渲染完成；工具不上传或返回文档内容。

现有配置的备份位于 `~/Library/Application Support/Glim/Integrations/Backups`，受私有目录权限保护，每个文件最多保留两份。取消关联只删除识别为 Glim 的条目，保留其他集成与设置。已有配置格式错误或存在同名冲突时不覆盖；可点击对应行的 Finder 按钮检查，再刷新状态。

各客户端的官方配置文档见[英文版参考链接](app-integrations.md#client-references)。
