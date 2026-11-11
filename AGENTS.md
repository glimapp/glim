# Glim development

## 快速迭代与验收

- 新任务先检查分支、已有改动和上游，fetch 后从最新上游快速创建独立 worktree，使用 `codex/` 分支；连续迭代复用该 worktree。保留原工作区，不为隔离任务复制整份编译缓存。
- 按可安装的小步迭代交付。必要验证保留：Rust 修改使用原生 `cargo check --workspace --all-targets --locked`，检查格式，构建包含 Swift 的完整应用，核对安装包并确认安装后的进程正常启动。开发构建可在原生 check 通过后使用 Blaze；不以 Blaze 替代 rustc 验证。
- 默认不运行完整测试、Clippy 或 UI smoke tests；用户明确要求、项目特定规则或实际故障需要时再执行对应检查。编译和启动成功不等同于交互验收通过。
- 必要验证通过后及时提交并推送任务分支，核对远端提交，然后立即打包／安装到 `/Applications/Glim.app` 并启动；不要等额外确认。无需发布正式 Release。失败时明确指出失败环节。
- 交付时直接说明已安装位置、分支／提交、已验证内容，以及用户在哪个界面、用哪些操作验收；列出尚未实际验证的关键交互。避免用开发过程日志代替验收说明。

- 修改代码前，必须先检查当前分支、工作区改动和上游关系，执行 `git fetch`，并将当前分支同步到最新上游代码后再开始修改；不能只检查本地缓存的远端分支。保留所有已有本地改动，不得通过 reset、覆盖或丢弃改动完成同步。存在分叉或冲突时先核对并处理，无法安全同步时明确说明阻塞，不得默默在旧代码上继续修改。连续任务中开始新的代码修改前也须重新核对上游状态。
- Product scope: a macOS file viewer (Apple Silicon and Intel) for documents, data, images and source code in one workspace. Ordinary UTF-8 files support basic notepad-style editing, automatic saving and explicit saving; images, document previews and unsupported files remain read-only.
- Platform strategy: maintain and release only macOS Apple Silicon and Intel. The repository is macOS-only: do not add non-macOS backends, platform branches, dependencies or release jobs without explicit user direction. Keep existing public release assets intact.
- Keep crate dependencies one-way: glim-app -> glim-services -> glim-core;
  glim-app may also depend on glim-core directly. Keep reader boundaries as ordinary modules unless a concrete independent version, dependency isolation or reuse requirement justifies another crate.
- glim-core must not depend on GPUI or perform I/O. Services do not import UI.
- Keep main.rs thin. Put application startup/actions in app/ and presentation in views/.
- Never perform file reads, directory scans, or blocking commands inside render methods.
  Schedule blocking work through the background executor and retain/cancel task handles.
- Keep GPUI entities alive across renders. Use theme tokens for UI colors.
- Define reader operations through `views/workspace/reader_actions.rs` (GPUI) or `native/ReaderActions.swift` (native). Inject format-specific ordered controls into the shared toolbar; reuse the same live actions for content context menus. Route native context clicks through `ReaderContextMenu` and build menus with `ReaderMenu`; do not add independent system-menu filtering or reader-specific right-click event monitors. Preserve source-editor editing menus and embedded web/media interactions.
- Do not impose arbitrary file-size limits or reject a supported file because of its size. Do not add file-size caps unless explicitly requested by the user. Support large files through streaming, paging, incremental processing and on-demand decoding. Bound active memory, buffers and concurrent work independently of total file size; keep operations cancellable. When changing an existing reader with a fixed file-size cap, replace that cap with scalable processing instead of preserving or increasing it.
- Manage application reader boundaries only in `assets/reader-limits.json`, with explicit units and effects. Rust, Swift and diagram JavaScript must consume the generated shared policy; do not duplicate numeric thresholds in readers or UI hints. Regenerate the bilingual inventories with `python3 scripts/generate-reader-limits.py --docs` after changing policy values. Format-validity rules and implementation invariants stay with their parsers.
- Keep editing minimal: typing, selection, undo/redo, automatic save and explicit save. Do not add LSP, external plugin loading, speculative abstraction layers, or empty placeholder modules. Internal reader modules follow docs/reader-modules.md; modularity alone does not justify more crates.
- Pin GPUI Kit; retain Cargo.lock. Enable extra features only when used.
- After requested code changes, build/package and install the updated local Glim.app by default, verify the installed bundle and launch it. Keep at most the two most recent installation backups. Do not automatically run tests or UI smoke tests; the user handles testing unless explicitly requested.
- Release requests follow the user's local acceptance of the installed build. Build production packages and verify their architecture, signatures, checksums and update metadata; do not repeat Cargo tests, Clippy or native updater tests locally or in release CI unless explicitly requested. A release request alone is not a validation request.
- 每次完成修改并完成适当验证后，自动提交并推送到当前分支对应的远端，不再另行询问；仅当用户明确要求保留在本地时跳过。只提交当前任务相关改动，保留无关工作，不强制推送。推送后核对远端提交与本地一致；若提交或推送失败，明确说明未完成的步骤与原因，不将本地提交视为已推送。
- When comprehensive validation is explicitly requested, use cargo fmt --all --check, cargo clippy --workspace --all-targets --locked -- -D warnings, cargo test --workspace --locked, and cargo build --locked. Routine iteration follows the necessary checks above.
- When UI smoke testing is requested, use macOS when available and report unverified interactions.
- Document changes to module boundaries in docs/architecture.md.
- When packaging Glim, replace `/Applications/Glim.app` directly, unless the user asks for packaging only. Never leave suffixed copies such as `Glim-backup.app` or `Glim-replaced.app` in `/Applications`. Keep recoverable backups only in `~/Library/Application Support/Glim/LocalInstallBackups`, retaining at most two. Verify the installed bundle and launch it.

- Keep documentation focused on current behavior, architecture and unfinished work. Put release notes in GitHub Releases; do not accumulate per-session operation logs, historical verification reports or personal filesystem inventories in the repository.

- Every GitHub Release must include equivalent English and Chinese release notes, including changes, support limitations, validation and installation requirements.

- Unless the user specifies otherwise, release requests increment the patch version (for example, 0.2.12 → 0.2.13).

<!-- antislop:start -->
## Anti-slop：UI 硬性规则

- 所有 UI 新增、修改和重构必须执行 anti-slop，读取可用的 `antislop` 与 `antislop-ui` 技能。项目默认使用 **during** 模式：在设计与实现过程中直接落实，交付前再检查；已有明确修改要求时，不额外询问模式或要求用户再次批准同一范围的修改。技能不可用时仍须执行本节规则，并说明缺失。
- 设计方向以 Glim 现有界面及用户要求为准：克制、内容优先、紧凑且可读。复用 GPUI/AppKit 控件、现有字体、图标与主题色；禁止引入无任务用途的装饰、大图标、大标题、灰底卡片、层层边框或阴影。新增视觉层级必须服务于具体信息或操作。
- 每个状态只保留必要信息和一个明确的主要操作。文件名、目标路径等关键内容优先；技术状态与低频反馈入口降级。删除重复解释、教学式废话和无用途的 placeholder；只有图标能明确表达的操作才使用纯图标，并提供悬停提示和无障碍名称。
- 查找等临时工具只在用户主动调用时浮出，关闭后完全隐藏，不占正文布局；不得擅自在右上角增加常驻搜索框、状态条或说明。加载与连接状态优先放在所属工作区、标签或操作控件内，不覆盖正文。
- 按钮必须有可辨认的操作形态、足够清晰的图标，以及悬停、焦点、按下、禁用和选中状态。颜色表达实际状态；选中使用原边框或局部填充，不叠加装饰性外框。保留键盘焦点可见性。
- 成功、复制、保存等瞬时反馈必须自动消失，优先原位短暂换图标；失败信息需可查看、复制或重试。异步状态变化不得撑开窗口、挤动正文或引起弹窗抖动；预留状态位置，必要时使用局部覆盖层。
- 交付前必须审查文案增量、信息层级、尺寸与间距、长文本和窄窗口、明暗主题、键盘与无障碍、图标资源完整性，以及空／加载／成功／失败状态。发现问题直接修复，不以审查报告替代实现；如实区分代码检查与实际交互验证。此规则不覆盖上文测试约定：未经用户明确要求，不自动运行测试或 UI smoke tests。
<!-- antislop:end -->
