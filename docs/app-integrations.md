# App integrations

[简体中文](app-integrations.zh-CN.md)

Open **Settings → App integrations**, then choose **Add integration** beside a detected application. A green dot and **Configured** mean the configuration has been written, not that the other application has established a live connection. Restart the target application or reload its MCP servers, and accept any tool approval it requires. Use the **Configured** menu to remove the integration.

| Client | User-level destination | Usage |
| --- | --- | --- |
| Codex | `~/.codex/config.toml`, detected `~/.codex-*` homes, `CODEX_HOME`, or a selected configuration folder | Adds Glim to **Open in** and makes it the default. |
| Claude Desktop | `~/Library/Application Support/Claude/claude_desktop_config.json` | Ask to open a local file or folder in Glim. |
| Claude Code | `~/.claude.json` or `$CLAUDE_CONFIG_DIR/.claude.json` | Same local tool. |
| Cursor | `~/.cursor/mcp.json` | Same local tool. |
| Gemini CLI | `~/.gemini/settings.json` | Same local tool. |
| GitHub Copilot | `$COPILOT_HOME/mcp-config.json` or `~/.copilot/mcp-config.json` | Copilot CLI and VS Code's Copilot harness. |
| GitHub Copilot · VS Code | `~/Library/Application Support/Code/User/mcp.json` | VS Code's Local harness / compatible older versions. |
| OpenCode | `$OPENCODE_CONFIG`, or `~/.config/opencode/opencode.jsonc` / `opencode.json` | Same local tool; uses OpenCode's `mcp` object and local command array. |

**Add configuration…** offers every supported client, then accepts a configuration file or folder. Selection adds an entry to the list; it does not modify that application's configuration until you choose **Add integration**.

For Codex, adding the integration sets `desktop.open-in-target-preferences.global` and any existing `perPath` choices to `custom:glim`. This applies to the selected instance, including projects with a saved editor choice, without changing macOS file associations. Existing integrations offer **Configured → Set as default** to apply the same preference. Removing the integration clears only preferences still pointing to Glim; it leaves subsequent editor choices intact and lets Codex choose its fallback. Restart Codex to load externally edited settings. The configured status confirms the saved entry, not that the running client has loaded its menu.

Discovery runs when opening Settings, returning to its window, or using Refresh. It examines recognized tool directories in the home directory, `~/.config`, and `~/Library/Application Support`, including suffix-named homes, account-manager folders and VS Code profiles up to three directory levels below each recognized root. It skips sessions, caches, projects and extension/plugin data. Configurations are deduplicated by tool and canonical file path, so multiple windows or application copies sharing one file have one entry. Multiple configurations of the same tool show their directory beneath the name.

Application discovery checks bundle identifiers under `/Applications` and `~/Applications`, including one level of grouping folders, so renaming a supported app does not hide it. It also checks the current environment, launchd environment and application `LSEnvironment` for documented configuration overrides such as `CODEX_HOME`, `CLAUDE_CONFIG_DIR`, `COPILOT_HOME`, `OPENCODE_CONFIG`, `OPENCODE_CONFIG_DIR` and `XDG_CONFIG_HOME`. It does not execute launcher scripts, inspect process secrets or scan the entire disk. A configuration hidden in an arbitrary script or unrelated location can still be added manually.

Project-specific settings, application policies and client version differences can override a user-level entry. Browser-only services and remote/cloud files are not supported by this local integration.

The MCP command is the installed `Glim.app/Contents/MacOS/glim --mcp`; no Node.js, Python, network listener or always-running service is installed. The `open_in_glim` tool accepts one absolute local path. A successful tool result confirms handoff to macOS, not completion of document rendering. It does not upload or return document contents.

Glim backs up existing configurations privately under `~/Library/Application Support/Glim/Integrations/Backups`, retaining the latest two copies per file. Removal deletes the recognized Glim entry and its Codex default references, leaving other integrations and settings intact. Existing malformed configurations or conflicting `glim` entries are left untouched; use the row's Finder action to inspect them, then refresh.

## Client references

- [Codex custom file handlers](https://learn.chatgpt.com/docs/config-file/config-advanced#add-custom-file-handlers)
- [Claude Desktop local servers](https://modelcontextprotocol.io/docs/develop/connect-local-servers)
- [Claude Code MCP](https://code.claude.com/docs/en/mcp)
- [Cursor MCP](https://cursor.com/docs/mcp)
- [Gemini CLI MCP](https://geminicli.com/docs/tools/mcp-server/)
- [GitHub Copilot CLI MCP](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers)
- [VS Code MCP destinations](https://code.visualstudio.com/docs/copilot/customization/mcp-servers)
- [OpenCode MCP](https://opencode.ai/docs/mcp-servers/) and [configuration scopes](https://opencode.ai/docs/config/)
