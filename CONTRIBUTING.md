# Help improve Glim / 参与 Glim

You can help by describing a task that is difficult today, sharing a sanitized example, improving documentation or contributing code. English and Chinese are welcome. Start with [topics](docs/topics.md) for current use cases and [compatibility](docs/file-format-support.md) for format details.

## Report a problem or request a capability

Search [existing issues](https://github.com/glimapp/glim/issues) first. Describe what you were trying to do, the Glim version, macOS version and Mac architecture, steps, and expected versus actual behavior. A small public or synthetic file is useful when a format matters.

For requests, explain who needs the feature and what they need to see or do. A workflow such as checking a route's coordinates is more useful than an extension name alone. Use [Discussions](https://github.com/glimapp/glim/discussions) for early ideas. Never upload private documents, credentials or proprietary model weights. Follow [private reporting](SECURITY.md) for vulnerabilities.

## Improve user documentation

Keep equivalent English and Chinese explanations in paired pages. Start with the user's task and available actions, then state limits. Link detailed compatibility rather than repeating a long format inventory. Keep the homepage banner and existing product captures. Generated reader-policy inventories come from `assets/reader-limits.json`; do not hand-edit their values. Documentation-only changes need content and link checks, not a product build.

## Contribute code

Keep PRs focused. Discuss substantial features in an issue before implementation. Read [remaining work](docs/TODO.md), [architecture](docs/architecture.md) and [build instructions](docs/releasing.md).

Dependencies run `glim-app → glim-services → glim-core`, with app also allowed to use core directly. Core has no I/O or GPUI; services have no UI. Keep filesystem work outside rendering, retain background-task handles, bound active resources and support cancellation without arbitrary total-file size caps.

Use the Rust version pinned in [CI](.github/workflows/ci.yml). Before submitting code, run:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --locked
```

Describe the user-visible change, checks performed and unverified behavior. Add a sanitized screenshot for UI changes and identify the platform actually tested. Supported targets are macOS Apple Silicon and Intel; other platform backends, CI and packaging are outside current scope. Contributions use the [MIT license](LICENSE) and follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## 中文说明

你可以反馈使用中的困难、提供脱敏样例、完善文档或贡献代码。先看[场景专题](docs/topics.zh-CN.md)与[格式兼容范围](docs/file-format-support.zh-CN.md)，了解当前可以做什么。

### 反馈使用问题和需求

先搜索已有 Issue。说明你是谁、正在做什么、Glim 和 macOS 版本、Mac 类型、操作步骤、期望与实际结果。格式问题尽量提供可公开的小型合成样例。需求要说清楚希望看到或操作哪些信息，例如“检查轨迹坐标”；只给扩展名不容易判断实际需求。早期想法可在 Discussions 交流，安全漏洞走[私密报告](SECURITY.md#中文说明)，不要提交私密文档、凭据或专有模型权重。

### 修改文档

中英文对应页保持含义一致，先讲场景与操作，再讲边界；兼容细节链接到格式表。保留首页头图及现有截图。阅读器数值清单从配置生成，不手改生成值。纯文档修改只需核对内容和链接，无需构建产品。

### 修改代码

大功能先讨论范围，改动保持聚焦。阅读[待完善事项](docs/TODO.zh-CN.md)、[架构](docs/architecture.zh-CN.md)及[构建说明](docs/releasing.zh-CN.md)。遵守上述单向依赖与后台 I/O 约定，约束活动资源并支持取消，避免按总文件大小拒绝支持的文件。

代码提交前执行上面的检查，说明真实用户行为、验证结果及未验证部分。界面变化附脱敏截图，注明实测平台。当前只维护 Apple Silicon 与 Intel macOS，贡献适用 MIT 许可证和社区行为准则。
