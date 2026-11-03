# Glim 文档

[English](README.md) · **简体中文** · [产品首页](../README.zh-CN.md) · [下载](https://github.com/glimapp/glim/releases/latest)

Glim 帮助你在 Mac 上查看工作、学习和项目中的各种文件，也能做基础文本修改。先找到自己的使用场景，再了解具体操作。

## 使用软件

| 你想了解什么 | 阅读入口 |
| --- | --- |
| 我适合用它做什么？ | [办公、阅读、多媒体、数据和专业场景](topics.zh-CN.md) |
| 第一次怎么用？ | [打开文件、工作空间、阅读与保存](user-guide.zh-CN.md) |
| 怎么选择安装包、首次打开？ | [下载与安装](../README.zh-CN.md#下载与安装) |
| 能否编辑、为什么预览有差异？ | [常见问题](faq.zh-CN.md) |
| 某个格式具体支持什么？ | [格式与兼容范围](file-format-support.zh-CN.md) |
| 如何更新、重启后恢复什么？ | [更新与恢复](updates.zh-CN.md) |
| 文件会上传吗、哪些内容会联网？ | [隐私与联网](privacy.zh-CN.md) |
| 哪些地方还在完善？ | [待办与验收缺口](TODO.zh-CN.md) |
| 使用辅助功能或遇到安全问题？ | [可访问性](../ACCESSIBILITY.md#中文说明) · [私密漏洞报告](../SECURITY.md#中文说明) |

## 参与项目与维护

普通使用无需阅读实现文档。想反馈问题或贡献代码时，从[贡献指南](../CONTRIBUTING.md#中文说明)开始。

| 维护资料 | 内容 |
| --- | --- |
| [架构](architecture.zh-CN.md) | 模块、工作空间、文件加载与各阅读器 |
| [阅读器模块契约](reader-modules.zh-CN.md) | 扩展入口、状态归属与依赖规则 |
| [构建与发布](releasing.zh-CN.md) | 工具链、打包、签名和双架构发行 |
| [更新实现](auto-update-design.zh-CN.md) | 更新信任链、安装保护与恢复机制 |
| [统计技术说明](telemetry.zh-CN.md) | 事件、指标及分析边界 |
| [阅读器策略清单](reader-limits.zh-CN.md) | 自动生成的分页和资源预算参考 |
| [资源与渲染](reader-resources.zh-CN.md) | 资源所有权、准入、回收、测量方法与覆盖边界 |
| [格式能力目录](format-catalog.zh-CN.md) | 生成的格式、图标和系统关联声明 |

文档对应当前源码；已发布版本的能力以 [Release 更新说明](https://github.com/glimapp/glim/releases)为准。

维护约定：待办及验收缺口集中在 `TODO.zh-CN.md`；架构、契约和策略文档描述当前设计与已落实行为。性能原始报告、会话记录和临时验证产物保存在仓库外；发布变更写入 GitHub Releases。
