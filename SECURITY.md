# Security policy

For local records, document network access and optional statistics, see [Privacy](docs/privacy.md). Use this page to report a vulnerability or check distribution trust.

## Supported versions

Security fixes target the latest published release. Older versions do not have a separate maintenance branch. Please update before checking whether an issue still occurs.

## Report a vulnerability privately

Use [Report a vulnerability](https://github.com/glimapp/glim/security/advisories/new) to contact the maintainers through GitHub's private vulnerability reporting. Do not open a public issue with exploit details or sensitive files.

Include the affected version and OS, impact, reproduction steps, and a minimal sanitized sample or proof of concept. Do not include credentials or other people's data. Please allow time to investigate before publishing details. This volunteer project does not promise a response deadline or offer a paid bounty.

File parsers, native preview helpers, and HTML rendering are relevant security surfaces. Ordinary crashes without a security impact can use the bug template.

## Distribution

Download binaries from [GitHub Releases](https://github.com/glimapp/glim/releases). Releases include SHA-256 checksums. Current releases target macOS Apple Silicon and Intel and use ad-hoc signing, without Developer ID notarization. Existing public assets remain historical downloads; only macOS receives maintained releases. Checksums verify file integrity, not publisher identity.

## 中文说明

文件处理、本地记录、文档联网和使用统计见[隐私说明](docs/privacy.zh-CN.md)。本页用于安全漏洞报告和发行包信任说明。

安全修复面向最新发行版本，旧版没有单独维护分支。请先升级，再确认问题是否仍然存在。

漏洞通过 [GitHub 私密漏洞报告](https://github.com/glimapp/glim/security/advisories/new)提交。提供版本、系统、影响、复现步骤和最小脱敏样例，不要公开利用细节、凭据或他人数据。项目由志愿者维护，不承诺响应期限，也没有付费漏洞奖励。

文件解析器、原生预览及 HTML 渲染都可能涉及安全问题；没有安全影响的普通崩溃可使用问题反馈表。

从 [GitHub Releases](https://github.com/glimapp/glim/releases)下载发行包。当前维护 Apple Silicon 与 Intel Mac，提供 SHA-256 校验，使用临时签名，尚无 Developer ID 签名与 Apple 公证。校验值用于核对完整性，单独不能认证发布者。历史公开附件保留，只有 macOS 持续维护。
