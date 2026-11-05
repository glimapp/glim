# 构建与发布

[English](releasing.md) · **简体中文** · [文档目录](README.zh-CN.md)

本文面向构建和发布维护者。安装与日常更新请看[安装帮助](../README.zh-CN.md#下载与安装)和[更新与恢复](updates.zh-CN.md)。

发布说明按用户场景写变化：说明谁会遇到、现在可以做什么，再列兼容限制、实际验证和安装要求。每次 GitHub Release 提供等价的中文与英文说明；未完成或未验证的能力明确标出。专题描述以[当前用户文档](topics.zh-CN.md)为依据，不以格式数量代替用户收益。

## 平台与准备

| 目标 | 要求 |
| --- | --- |
| macOS Apple Silicon — `aarch64-apple-darwin` | Rust、包含 Swift 的 Apple Command Line Tools、Python 3 |
| macOS Intel — `x86_64-apple-darwin` | 同样的 macOS 工具及目标工具链 |

Rust 版本以 [CI](../.github/workflows/ci.yml) 固定值为准。当前仅维护并打包 macOS Apple Silicon 与 Intel；构建脚本拒绝非 macOS 目标，既有公开附件继续保留。

## 开发与验证

```sh
cargo run --locked -- /path/to/project
```

开发完成后，先构建、打包并安装到本地，由用户手动验收。用户确认后提出发布请求时，从已验收源码构建正式包，并校验安装包完整性和更新元数据。发布时不在本地或 CI 重复运行 Cargo 测试、Clippy 或原生更新器测试。

仅在明确请求验证时执行以下命令：

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --locked
```

界面由用户手动验收；仅在明确请求时运行自动烟测，发布说明中写明未验证交互。纯文档修改检查内容／链接，不需要构建产品。操作日志与历史测试报告不放入仓库文档。

## 打包

保持 Cargo 工作区版本、lockfile 中工作区包版本与 `CFBundleShortVersionString` 一致。每次正式发布递增 `CFBundleVersion`，不替换已发布的同版本包。

```sh
scripts/bundle-macos.sh release
scripts/package-dmg.sh arm64
scripts/package-dmg.sh x86_64
```

未配置的 bundle 命令生成手动下载构建，不是启用更新器的发行包。默认输出 `dist/Glim.app`；分架构包输出 `dist/release-<arch>/Glim.app`、`Glim-<version>-macos-<arch>.dmg` 和 `.sha256`。`GLIM_BUNDLE_DIR` 覆盖路径，兼容旧 `GLIMPSE_BUNDLE_DIR`。

`target/` 与 `dist/` 是忽略的产物。正常构建使用已提交的图标及离线图表资源，不需要 Node.js。修改图表引擎源码／依赖时，在 `scripts/markdown/` 执行 `npm ci --ignore-scripts` 和 `npm run bundle`，同时提交生成资源及许可证。

macOS 先签嵌套可执行文件／bundle／framework，再签外层 app。`GLIM_CODESIGN_IDENTITY` 选择身份，默认 `-` 是 ad-hoc 签名，不是 Developer ID 签名或公证。最低系统版本从支持的 Mach-O 部署指令推导，原生 Swift 桥接至少要求 macOS 11。应核对实际包架构，不能一概按 ARM64 验收。

本地安装通常先备份旧 app，再替换、核对安装内容并启动；若任务是测试应用内升级，则保留旧版。首次打开的 Gatekeeper 步骤见[安装帮助](../README.zh-CN.md#macos)。

<a id="automatic-updates"></a>
## 自动更新配置

macOS 发行包使用固定 Sparkle 2.10.0，由 `scripts/fetch-sparkle.sh` 校验下载。各工作区共用复用的居中更新窗口。macOS 更新器从 0.1.12 开始提供，0.1.11 及更早版本需要先手动安装一次；未配置的本地构建保留 GitHub 下载入口。

### 一次性签名配置

1. 执行 `scripts/fetch-sparkle.sh`。
2. 使用 `target/sparkle/2.10.0/bin/generate_keys --account Glim` 生成或读取钥匙串密钥，保留受保护的离线备份。
3. 仓库变量 `GLIM_UPDATE_PUBLIC_KEY` 设置为 32 字节 Ed25519 公钥的 Base64 编码。
4. 创建 GitHub Environment `release`，设置可信维护者审批及允许的发布 tag。使用 `generate_keys --account Glim -x <受保护文件>` 导出，将 32 字节私钥 seed 存为环境 secret `GLIM_UPDATE_PRIVATE_KEY`。
5. 私钥不得提交、放入应用包或输出到日志／Issue。后续版本沿用既有密钥；没有支持的迁移方案时，换钥会破坏既有信任链。

本地启用更新器打包只需要公钥：

```sh
GLIM_ENABLE_UPDATER=1 GLIM_UPDATE_PUBLIC_KEY='<public-key>' scripts/package-dmg.sh arm64
```

脚本启用 `macos-updater`，内嵌 Sparkle 和许可证，写入分架构 feed／公钥及严格校验配置，并签名嵌套组件。更新签名凭据与 Apple 公证不同。

### 准备与发布版本

1. 提交最终源码及一致版本，推送稳定 `vX.Y.Z` tag。
2. 等待同一提交、同一版本标签的两个 `macOS builds` 工作全部成功。ARM 与 Intel 分别构建 release，并校验安装包架构、签名和发行元数据；打包流程不编译或运行测试，也不运行 Clippy。
3. 从发布 tag 手动运行 `Prepare signed release draft`，输入同一 tag 和含 `## English`／`## 中文` 的等价说明，并批准受保护环境。`bootstrap` 只用于前一版没有 appcast 的首次更新器发布，后续关闭。
4. 工作流下载同一提交的成功产物，核对全部 SHA-256、两种 DMG 的身份／公钥／feed／架构／build，验证上一版 feed，保留兼容历史并生成、验证新签名 feed 与安装包。
5. 将两个 Mac 安装包、两个校验文件和两个 appcast（共六个附件） 上传草稿，回下载核对字节，不修改已公开 Release，不覆盖既有资产。
6. 检查完整草稿，公开并标记 Latest，再检查两个公开 feed 地址、签名及版本／build。CI 成功本身不会自动公开版本。

feed 地址是 `/releases/latest/download/appcast-macos-<arch>.xml`，安装包使用固定 `/releases/download/<tag>/Glim-<version>-macos-<arch>.dmg`。运行时 macOS 客户端不需要 GitHub Token，也不查询 Releases API。README 下载徽标与曲线共用每日快照，仅统计公开 `.dmg` 安装包下载（包含 DMG 的自动更新及核验下载），不计 feed、校验文件、ZIP 等其他附件，不等于用户数或安装量。

### 发布构建与缓存

CI 的 Cargo 步骤统一使用 `--release --target <arch> --features macos-updater --locked`，不构建 dev／debug，不在打包前执行 `cargo clean`。CI 保持 release 优化，设置 `CARGO_PROFILE_RELEASE_LTO=false` 和 `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16` 以缩短链接／代码生成；本地默认 release profile 不受影响。

普通分支 push 不触发 macOS 构建；ARM／Intel 正式构建只在推送 `v*` 版本 tag 或明确手动运行时执行。发布前如需预热默认分支缓存，可在 main 上手动运行 `macOS builds`，该操作可选且不上传安装包。版本 tag 的 push 生成两种 DMG，并校验安装包完整性和元数据；不同 tag 的缓存不能直接互用，但可以恢复默认分支可见缓存。同一分支重复手动运行会取消过时任务，tag 构建不会自动取消。签名发布工作流只接受同一 tag、同一提交的成功 push 构建。GitHub 托管的 CodeQL 安全扫描独立于该构建触发策略。

本地优化包的遥测默认仍为 debug。只有官方 tag push 设置 `GLIM_DISTRIBUTION=github-release`，由构建来源条件决定 release 路由，CI 在打包前通过 `--build-info` 核验可执行文件；手动预热构建保持 debug。正常应用升级保留应用数据库中的安装标识。见[遥测渠道](telemetry.zh-CN.md#开发与正式渠道)。

缓存按架构和工具链／Cargo 配置区分，保留依赖及工作区编译产物；Sparkle 仅缓存固定压缩包，每次从验过摘要的字节重新解压。DMG 已压缩，上传不再重复压缩，构建附件保留七天。Job Summary 记录缓存命中和磁盘占用，Actions 步骤记录耗时。冷缓存、工具链／依赖变化、排队和网络波动都会影响总耗时，应按阶段实测，不假定固定发布时长。安装包校验不代表任一架构的阅读器交互已经验收。

要单独试验交叉编译，可在分支上手动运行 `macOS builds`，设置 `intel-cross-compile=true`。该模式只在 Apple Silicon runner 上构建并打包 `x86_64-apple-darwin`，再由 Intel runner 校验 DMG 摘要、架构、签名并执行 `--build-info`。试验上传保留七天的附件，不创建 Release。首次运行使用独立冷缓存，还应对比重复运行的耗时。普通版本 tag 构建继续使用原生 runner 及安装包校验；执行 `--build-info` 不代表阅读器交互已验证。

### 隔离更新验证

```sh
scripts/fetch-sparkle.sh
export GLIM_SPARKLE_DIR="$PWD/target/sparkle/2.10.0"
cargo clippy --workspace --all-targets --features macos-updater --locked -- -D warnings
cargo test --workspace --features macos-updater --locked
scripts/test-updater.sh
python3 scripts/test-updater-install.py
```

专项检查覆盖原生批准／取消、两种部署指令格式及未签名 Intel 嵌套包签名。安装测试使用临时 app／密钥和 loopback feed，检查篡改拒绝、手动重启与正常退出安装，不替换用户已安装 app 或生成正式密钥；它不能证明所有实机／管理员授权场景。

参见[更新行为与恢复](auto-update-design.zh-CN.md)、[架构](architecture.zh-CN.md)和[未完成事项](TODO.zh-CN.md)。
