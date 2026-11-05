<p align="center"><img src="assets/branding/repository-banner.svg" alt="Glim — 文档、数据和代码，一个 Mac 工作区。免费开源，支持 Apple Silicon 与 Intel。" width="100%" /></p>

# Glim · 在一个 Mac 工作区里查看你的文件

Glim 是免费开源的 Mac 文件查看器。收到一份合同、一个资料包，或者打开自己的项目文件夹，都可以把文档、图片、表格、音视频和代码放在同一个窗口里查看。普通文本还能顺手修改并保存。

<p align="center">免费开源 · macOS · Apple Silicon 与 Intel · <a href="README.md">English</a> / <b>简体中文</b></p>

<p align="center"><a href="https://glimapp.github.io/">Website / 官网</a> · <a href="https://github.com/glimapp/glim/releases/latest"><strong>下载 Glim →</strong></a></p>

<p align="center"><a href="https://github.com/glimapp/glim/releases/latest">最新发布 · 更新说明与校验文件</a> · <a href="#下载与安装">安装帮助</a></p>

<p align="center"><sub>Mac 下载包目前尚未经过 Apple 公证，首次打开可能需要在系统设置中放行。<a href="#macos">查看步骤</a></sub></p>

<p align="center">
  <a href="https://github.com/glimapp/glim/releases"><img src="https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2Fglimapp%2Fglim%2Frefs%2Fheads%2Fcodex%2Fdownload-stats%2Fdownloads-badge.json&amp;style=flat-square&amp;label=DMG%20%E4%B8%8B%E8%BD%BD%E9%87%8F" alt="DMG 安装包下载量" /></a>
  <a href="https://github.com/glimapp/glim/actions/workflows/ci.yml"><img src="https://github.com/glimapp/glim/actions/workflows/ci.yml/badge.svg" alt="桌面版构建状态" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="MIT 许可证" /></a>
  <a href="https://github.com/glimapp/glim/stargazers"><img src="https://img.shields.io/github/stars/glimapp/glim?style=social" alt="GitHub Star 数量" /></a>
</p>

## 你平时会用它看什么？

办公时，可能要一起看合同、表格和演示文稿；学习时，手边有 PDF、电子书和笔记；做项目时，资料里又夹着图片、数据和代码。Glim 把这些文件放在同一个工作区，按你手里的事情来组织。

| 专题 | 适合谁、用来做什么 |
| --- | --- |
| [办公文件](docs/topics.zh-CN.md#office) | 职场人士查看合同、报告、表格和演示文稿 |
| [阅读与学习](docs/topics.zh-CN.md#reading) | 学生、研究者和阅读爱好者阅读 PDF、电子书、漫画和笔记 |
| [图片与设计素材](docs/topics.zh-CN.md#images) | 查看照片、动画、SVG 和字体，复制图片或提取图片文字 |
| [音视频与字幕](docs/topics.zh-CN.md#media) | 播放本地音视频，按文件夹连续播放，单独查看和修改字幕 |
| [笔记与思维导图](docs/topics.zh-CN.md#notes) | 写 Markdown、查看 XMind 和流程图，整理说明与思路 |
| [表格与数据](docs/topics.zh-CN.md#data) | 运营、分析师和开发者查看 CSV 表格、JSON 数据树与交互图 |
| [数据库](docs/topics.zh-CN.md#database) | 浏览本地 SQLite 的表、结构和记录，使用只读快照 |
| [开发者](docs/topics.zh-CN.md#developer) | 一起阅读源码、网页、配置、日志和技术文档，做基础文本修改 |
| [2D 图形与 3D 模型](docs/topics.zh-CN.md#graphics) | 查看 SVG、技术图表及 OBJ/STL 的静态网格投影 |
| [GIS 与轨迹](docs/topics.zh-CN.md#gis) | 检查 GeoJSON 的字段、GPX/KML 的坐标和原文 |
| [AI 与科研文件](docs/topics.zh-CN.md#research) | 检查模型元数据、部分 NumPy 数值及 Notebook 数据结构 |
| [压缩包与资料交接](docs/topics.zh-CN.md#archives) | 浏览 ZIP、解压到新目录，或查看 TAR 目录和压缩文本 |

[按专题了解具体用法与限制](docs/topics.zh-CN.md) · [查询某种文件格式](docs/file-format-support.zh-CN.md)

## 打开一个文件夹，把相关资料放在一起

<p align="center"><img src="assets/readme/workspace.png" alt="左侧文件夹列表，右侧阅读 Markdown 报告，顶部标签保留相关图片和文档。" width="100%" /></p>

你可以把一次工作的资料作为一个工作区：在左侧找文件，在顶部标签之间切换。一个窗口可以保留多个工作区，文件标签也可以拆到独立窗口。重启后可恢复窗口、工作区和文件标签；通用滚动位置与媒体播放进度尚不完整恢复。

## 需要时，接着查看资料里的数据

<p align="center"><img src="assets/readme/sqlite.png" alt="选择 SQLite 数据表，筛选记录，并使用排序和分页查看数据。" width="100%" loading="lazy" /></p>

例如，报告旁边还有一份本地数据库。在 Glim 中打开 SQLite，选择表、筛选和排序记录，双击单元格阅读完整值。查看使用独立快照，源数据库保持只读。

## 第一次使用

1. 下载适合自己 Mac 的安装包，将 Glim 拖入“应用程序”。
2. 按 ⌘O 打开文件，或按 ⌘⇧O 选择文件夹。已有资料可以直接打开。
3. 在标签之间切换；支持的内容可以切换预览与原文。阅读控件通常在内容底部附近，部分阅读器需要将鼠标移过去才会显示。
4. 需要记点东西时按 ⌘N；需要修改普通文本时进入原文。已有文本文件修改后会自动写回原文件，⌘S 也可以保存。

[完整使用指南](docs/user-guide.zh-CN.md) · [常见问题](docs/faq.zh-CN.md) · [文档目录](docs/README.zh-CN.md)

## 使用时了解这些边界

- 文档、图片、数据表、数据库和思维导图预览以查看为主。普通 UTF-8 文本支持基础编辑、撤销重做、自动保存和显式保存。
- Office 的复杂排版可能有差异，Excel 显示已保存的公式结果，不重新计算。需要精确排版或专业编辑时，可交给相应应用。
- GIS 当前展示数据和坐标；3D 当前提供静态投影。它们的具体范围见各专题。
- 文件在本地打开。Markdown 的远程图片、HTML 的脚本与网络资源，以及更新和启用的使用统计可能联网，见[隐私与联网说明](docs/privacy.zh-CN.md)。
- 支持 Apple Silicon 和 Intel Mac。安装包尚未经过 Apple 公证，首次打开步骤见下文。

## 下载与安装

在[最新 Release 页面](https://github.com/glimapp/glim/releases/latest)的 **Assets** 中，按设备选择对应安装包；版本号、更新说明和校验文件以该页面为准。

| 你的设备 | 安装包文件名后缀 |
| --- | --- |
| Mac · Apple Silicon（M 系列） | `-macos-arm64.dmg` |
| Mac · Intel | `-macos-x86_64.dmg` |

不确定 Mac 的类型？打开 **苹果菜单 → 关于本机**：看到 Apple M 系列芯片就选 Apple Silicon，看到 Intel 处理器就选 Intel。

### macOS

打开 DMG，将 Glim 拖入“应用程序”。目前使用 ad-hoc 签名，尚无 Developer ID 签名与 Apple 公证。若系统拦截，先尝试打开一次，再到 **系统设置 → 隐私与安全性 → 仍要打开**。请确认安装包来自本项目发布页，无需关闭系统安全保护。[Apple 官方说明](https://support.apple.com/zh-cn/102445)

[更新说明与 SHA-256 校验文件](https://github.com/glimapp/glim/releases/latest) · [历史版本](https://github.com/glimapp/glim/releases) · [从源码构建](docs/releasing.zh-CN.md)

## 参与项目

[报告问题](https://github.com/glimapp/glim/issues/new?template=bug_report.yml) · [申请格式或功能](https://github.com/glimapp/glim/issues/new?template=feature_request.yml) · [讨论与交流](https://github.com/glimapp/glim/discussions)，欢迎中文和英文。

[贡献指南](CONTRIBUTING.md#中文说明) · [构建说明](docs/releasing.zh-CN.md) · [未完成计划](docs/TODO.zh-CN.md) · [安全政策（英文）](SECURITY.md) · [MIT License](LICENSE)

## 贡献者

感谢每一位帮助 Glim 变得更好的贡献者。

<a href="https://github.com/glimapp/glim/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=glimapp/glim" alt="Glim 贡献者头像墙" />
</a>

## Star 增长曲线

<a href="https://star-history.com/#glimapp/glim&amp;Date">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=glimapp/glim&amp;type=Date&amp;theme=dark" />
    <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=glimapp/glim&amp;type=Date" />
    <img src="https://api.star-history.com/svg?repos=glimapp/glim&amp;type=Date" alt="Glim 在 GitHub 上的 Star 增长曲线" width="100%" />
  </picture>
</a>

## 下载量增长曲线

<a href="https://github.com/glimapp/glim/tree/codex/download-stats">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/glimapp/glim/refs/heads/codex/download-stats/downloads-dark.svg" />
    <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/glimapp/glim/refs/heads/codex/download-stats/downloads.svg" />
    <img src="https://raw.githubusercontent.com/glimapp/glim/refs/heads/codex/download-stats/downloads.svg" alt="Glim DMG 安装包累计下载量曲线" width="100%" />
  </picture>
</a>

仅统计 GitHub 公开 Release（含预发布）中 **.dmg 安装包**的累计下载次数，徽标与曲线使用同一份每日快照。纯 DMG 曲线从 **2026-10-30** 开始记录，累计值包含旧版本 DMG 至今的下载次数。此前的全附件合计无法按文件类型还原，已[单独归档](https://github.com/glimapp/glim/blob/codex/download-stats/downloads-all-assets-legacy.json)。不计更新清单、校验文件、ZIP 等附件；下载次数不等于独立用户数或安装量。
