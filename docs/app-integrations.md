# App integrations

[简体中文](app-integrations.zh-CN.md)

Open **Settings → App integrations**, then choose **Add integration** beside a detected application. A green dot and **Configured** mean the configuration has been written, not that the other application has established a live connection. Restart the target application or reload its MCP servers, and accept any tool approval it requires. Use the **Configured** menu to remove the integration.

| Client | User-level destination | Usage |
| --- | --- | --- |
| Codex | `~/.codex/config.toml`, detected `~/.codex-*` homes, `CODEX_HOME`, or a selected configuration folder | Choose Glim from **Open in**. |
| Claude Desktop | `~/Library/Application Support/Claude/claude_desktop_config.json` | Ask to open a local file or folder in Glim. |
| Claude Code | `~/.claude.json` | Same local tool. |
| Cursor | `~/.cursor/mcp.json` | Same local tool. |
| Gemini CLI | `~/.gemini/settings.json` | Same local tool. |
| GitHub Copilot | `$COPILOT_HOME/mcp-config.json` or `~/.copilot/mcp-config.json` | Copilot CLI and VS Code's Copilot harness. |
| GitHub Copilot · VS Code | `~/Library/Application Support/Code/User/mcp.json` | VS Code's Local harness / compatible older versions. |
| OpenCode | `$OPENCODE_CONFIG`, or `~/.config/opencode/opencode.jsonc` / `opencode.json` | Same local tool; uses OpenCode's `mcp` object and local command array. |

OpenCode discovery honors `XDG_CONFIG_HOME`. **Add Codex configuration…** selects another folder containing `config.toml`; it does not immediately modify that configuration. Discovery checks conventional application/configuration locations, not every file on the disk. Project-specific settings, application policies and client version differences can override a user-level entry. Browser-only services and remote/cloud files are not supported by this local integration.

The MCP command is the installed `Glim.app/Contents/MacOS/glim --mcp`; no Node.js, Python, network listener or always-running service is installed. The `open_in_glim` tool accepts one absolute local path. A successful tool result confirms handoff to macOS, not completion of document rendering. It does not upload or return document contents.

Glim backs up existing configurations privately under `~/Library/Application Support/Glim/Integrations/Backups`, retaining the latest two copies per file. Removal deletes only the recognized Glim entry, leaving other integrations and settings intact. Existing malformed configurations or conflicting `glim` entries are left untouched; use the row's Finder action to inspect them, then refresh.

## Client references

- [Codex custom file handlers](https://learn.chatgpt.com/docs/config-file/config-advanced#add-custom-file-handlers)
- [Claude Desktop local servers](https://modelcontextprotocol.io/docs/develop/connect-local-servers)
- [Claude Code MCP](https://code.claude.com/docs/en/mcp)
- [Cursor MCP](https://cursor.com/docs/mcp)
- [Gemini CLI MCP](https://geminicli.com/docs/tools/mcp-server/)
- [GitHub Copilot CLI MCP](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers)
- [VS Code MCP destinations](https://code.visualstudio.com/docs/copilot/customization/mcp-servers)
- [OpenCode MCP](https://opencode.ai/docs/mcp-servers/) and [configuration scopes](https://opencode.ai/docs/config/)
