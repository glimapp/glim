# 待办与验收缺口

[English](TODO.md) · **简体中文** · [文档目录](README.zh-CN.md)

本文按实现、测试和测量入口区分已完成基础、剩余工程和运行验收。未勾选条目表示其完成条件尚未全部满足；其中已实现的子项不需要重做。测试代码或测量脚本存在，不代表相关实机场景已经验收。候选能力不承诺发版时间。

## 已实现的基础

| 能力 | 当前实现与代码入口 |
| --- | --- |
| 格式注册与三层依赖 | 格式目录生成消费者，有序提供者注册与集中阅读器装配；保留 `app → services → core`。[目录](../assets/format-catalog.json)、[注册](../crates/glim-services/src/documents/registry.rs)、[装配](../crates/glim-app/src/views/reader/construction.rs)、[架构检查](../scripts/check-architecture.py)。 |
| 部分阅读器展示隔离 | OPML、交互图、JSON、CSV／地理表格、SQLite、书籍已有纯数据／宿主接口；表格、SQLite、书籍分别发出语义请求，由适配器负责准备及持久化。[模块契约](reader-modules.zh-CN.md)、[表格](../crates/glim-app/src/views/reader/csv/view.rs)、[SQLite](../crates/glim-app/src/views/reader/sqlite/view.rs)、[书籍](../crates/glim-app/src/views/reader/books/view.rs)。 |
| 编辑与会话所有权 | 编辑会话与保存基线独立于展示，源码／预览共享编辑状态；工作空间保存活动选择，书籍保存章节／位置，草稿已有后台检查点。[编辑会话](../crates/glim-app/src/app/edit_session.rs)、[文档会话](../crates/glim-app/src/app/document_session.rs)、[草稿](../crates/glim-services/src/drafts.rs)。 |
| 共享资源与任务准入 | 文本／十六进制页、表格单元格、结构树、结构化交互图最终输出与遍历数组、缩略图几何、PNG／GIF 主要像素缓冲已有分配前准入；多类阅读器支持隐藏释放。[资源池](../crates/glim-core/src/resources.rs)、[图投影](../crates/glim-services/src/graph/projection.rs)、[像素解码](../crates/glim-services/src/media/raster.rs)、[覆盖边界](reader-resources.zh-CN.md)。 |
| 部分增量处理 | 源码缩略图共享 Rope 和未变化几何块，只重算变化行；Markdown 对安全的普通文字编辑局部重解析，并保留完整解析回退。[缩略图](../crates/glim-app/src/views/minimap/geometry.rs)、[Markdown](../vendor/gpui-base/src/text/format/markdown/edit.rs)。 |
| 图表缓存 | 展示层与服务层已有进程内复用，成功结果已有磁盘后备；包含源／引擎／环境失效、损坏校验、容量淘汰、原子写入和取消。文件名不保存源码，PNG 仍含文档内容；清理方式见资源文档。[展示缓存](../crates/glim-app/src/views/reader/diagrams.rs)、[磁盘缓存](../crates/glim-services/src/markdown_preview/disk_cache.rs)、[清理与限制](reader-resources.zh-CN.md#图表缓存与命中率)。 |

## 架构与性能待办

约束：阅读器优先使用普通模块。独立 crate 需要真实的独立版本、依赖隔离或复用需求；不为模块数量或扩展名数量拆包。新增格式和使用场景继续组合已有能力，保留统一动作、编辑会话、资源所有权及取消机制。

### 阅读器模块化

- [ ] **M1：剩余阅读器边界。** 媒体、压缩包、分页、源码／排版与原生适配仍直接使用服务载荷、应用状态或调度入口；其中部分仍在一个模块内组合展示与准备。按实际耦合逐个分离，保留适配器承担后台任务，向展示注入必要的数据和宿主操作，不机械地增加接口或 crate。
  - 完成条件：相应展示不承担文件 I/O、持久化或后台任务编排；能用代表性准备数据及注入宿主独立构造，架构检查覆盖新增边界。
  - 入口：[媒体](../crates/glim-app/src/views/reader/media.rs)、[压缩包](../crates/glim-app/src/views/reader/archive.rs)、[分页](../crates/glim-app/src/views/reader/paged.rs)、[源码](../crates/glim-app/src/views/reader/source.rs)、[排版](../crates/glim-app/src/views/reader/markup.rs)、[原生](../crates/glim-app/src/views/reader/native.rs)。
- [ ] **M2：跨阅读器生命周期契约。** 表格已有交接／隐藏恢复测试；SQLite 已覆盖取消、筛选排序、隐藏刷新、过期结果及重试；书籍已覆盖偏好恢复中断、选区、章节替换、待写排空、关闭后写完和窗口重绑。其余阅读器及完整工作空间移动仍缺统一的代表性覆盖。
  - 完成条件：覆盖激活／隐藏、刷新、取消、失败重试、关闭及跨窗口移动；保留编辑会话、撤销、未保存内容和必要导航状态，过期结果不覆盖新版本。区分自动契约测试与物理输入验收。
  - 入口：[生命周期接口](../crates/glim-app/src/views/reader/contract.rs)、[表格测试](../crates/glim-app/src/views/reader/csv/view.rs)、[SQLite 测试](../crates/glim-app/src/views/reader/sqlite.rs)、[书籍测试](../crates/glim-app/src/views/reader/books.rs)。

### 资源覆盖与压力恢复

- [ ] **R1：剩余可控分配准入。** JSON／JSON5 的解析器值树、结构化数据转图的输入中间表示、图布局、书籍 HTML／元数据、编码预览／SVG 中间缓冲，以及 SQLite／地理解析暂存仍未完整准入。最终树／图结果已准入，不能据此推断准备峰值也受预算约束。
  - 完成条件：可控工作缓冲在分配／扩容前取得预算，计入新旧缓冲重叠；所有者跨任务与发布交接持续持有额度。逐项替换绕过准入的工作缓冲记账，保留必要的状态记账及完整源后备，不以文件大小拒绝支持的文件。
  - 入口：[JSON](../crates/glim-services/src/json.rs)、[图准备](../crates/glim-services/src/graph.rs)、[书籍](../crates/glim-services/src/books/mod.rs)、[SVG](../crates/glim-services/src/svg_preview.rs)、[SQLite](../crates/glim-services/src/sqlite/browser.rs)、[地理表格](../crates/glim-services/src/geo.rs)。
- [ ] **R2：原生与辅助进程资源。** 原生图片、OCR 和复制已有共享所有权与租约记账；Rust 桥接仍使用 `track`，没有字节预算准入。准备／转换中间缓冲及独立辅助进程、ImageIO、WebKit、Quick Look、GPUI 图集、SQLite 引擎的覆盖仍有限。
  - 完成条件：为可控原生工作集接入准入和可取消交付，运行中的消费者退出前不提前释放占用；配套系统内存与辅助进程观测，明确框架内存边界，不将应用记账称作进程内存硬上限。
  - 入口：[原生所有权](../native/ReaderResources.swift)、[Rust 桥接](../crates/glim-app/src/views/reader/native_bridge.rs)、[原生资源测试](../native/tests/ReaderResourcesTests.swift)。
- [ ] **R3：跨活动阅读器的压力恢复。** 资源池已有缓存淘汰，部分阅读器隐藏时会释放工作数据；`reserve_working` 容量不足时返回可重试错误，尚未协调活动阅读器释放可重建工作缓冲并恢复请求。
  - 完成条件：多窗口和单个大操作在有限工作内存下仍能前进；优先保护未保存内容与必要交互状态，统一释放、重试和重建，取消中的工作实际退出后才归还占用。
  - 入口：[准入与缓存回收](../crates/glim-core/src/resources.rs)、[任务调度](../crates/glim-services/src/work/scheduler.rs)、[SQLite 隐藏恢复](../crates/glim-app/src/views/reader/sqlite.rs)、[书籍隐藏恢复](../crates/glim-app/src/views/reader/books.rs)。

### 增量处理与性能证据

- [ ] **P1：剩余全量扫描、克隆与布局。** 缩略图增量路径已实现。Markdown 局部重解析仍克隆块集合、复制源文本并修正后续源码范围，结构性编辑保留完整解析；压缩包切换目录仍在界面侧遍历全部条目并重建列表。单个大排版块的成本也需测量。
  - 完成条件：先测量再减少这些路径的重复工作，必要时引入索引或后台准备；扩展增量范围时与完整解析对照，保留引用、列表、围栏、扩展节点及源码映射语义。报告规模、修改位置和延迟分位数。
  - 入口：[Markdown 增量实现](../vendor/gpui-base/src/text/format/markdown/edit.rs)、[排版](../vendor/gpui-base/src/text/text_view.rs)、[压缩包列表](../crates/glim-app/src/views/reader/archive.rs)、[Markdown 基准](../crates/glim-app/benches/markdown_edit.rs)、[缩略图基准](../crates/glim-app/benches/minimap_edit.rs)。
- [ ] **P2：端到端阅读器测量。** 已有解析、像素、结构树等局部基准，以及源码阅读器真实窗口输入／滚动脚本；尚不能据此覆盖工作空间保存协调、分页、异步文档／数据预览、书籍、图表和原生阅读器的完整路径。
  - 完成条件：分别记录首个可读内容、首屏、输入／滚动阻塞及排队／准备耗时，检测真实内容就绪；保留固定夹具、机器／工具链／源码信息和前后原始样本。测量产物放在仓库外。
  - 入口：[真实窗口脚本](../scripts/measure-source-reader.py)、[测量装配](../crates/glim-app/src/app/reader_measurement.rs)、[测量范围](reader-resources.zh-CN.md)。
- [ ] **P3：真实缓存收益。** 进程内和磁盘缓存已实现；现有 `diagram_cache` 基准只测准备好的 PNG 查找／发布，磁盘缓存测试覆盖重开、失效、损坏、回收和取消，仍缺包含真实渲染引擎及首屏的完整收益对照。
  - 完成条件：覆盖首次打开、关闭标签再开、退出重开、滚出／返回、失效及资源压力；分别报告展示／服务／磁盘命中、未命中、等待、回收、实际避免的准备工作及首屏延迟，包含首次环境指纹初始化成本。
  - 入口：[缓存基准](../crates/glim-services/benches/diagram_cache.rs)、[磁盘缓存及测试](../crates/glim-services/src/markdown_preview/disk_cache.rs)、[展示缓存](../crates/glim-app/src/views/reader/diagrams.rs)。
- [ ] **P4：实机运行验收。** 发布 CI 在 Apple Silicon 执行工作区测试和原生更新器检查，两种架构都构建并校验安装包；Intel 实机交互、完整 UI 回归、多窗口和资源压力不由这些检查证明。
  - 完成条件：按下方场景执行相关自动回归及 Apple Silicon／Intel 实机操作，记录实际覆盖；未执行或未通过的场景继续保留。
  - 入口：[发布 CI](../.github/workflows/ci.yml)、[原生预览检查](../scripts/test-native-preview.sh)、[隔离安装回归](../scripts/test-updater-install.py)。

建议顺序：先逐个完成 **M1＋M2**，再推进 **R1＋R3**；涉及原生路径时同步落实 **R2**，每个模块伴随 **P1～P3** 的针对性测量，最后按 **P4** 验收。新增格式与场景能力单独排期。

## 场景能力候选

这些是产品能力差距，不是完成内部模块化的前提。具体格式限制见[格式支持](file-format-support.zh-CN.md)。

| 场景 | 已有能力与剩余差距 | 代码入口 |
| --- | --- | --- |
| 办公 | 已有分页表格、缓存值及 Word 阅读视图；复杂排版、图表／嵌入对象、公式重算和完整开放文档阅读仍有限。 | [工作簿](../native/WorkbookPreview.swift)、[表格准备](../crates/glim-services/src/spreadsheet.rs)、[Word](../native/WordPreview.swift) |
| 阅读 | 已有章节／目录、插图、书签及位置保存；未完整还原 EPUB 出版者 CSS 和固定版式。 | [书籍解析](../crates/glim-services/src/books/mod.rs)、[阅读状态](../crates/glim-core/src/books.rs) |
| 继续工作 | 工作空间／标签和书籍位置已有持久化，草稿已有检查点与恢复；通用滚动、光标、媒体进度恢复尚未统一，异常退出／多实例下的可靠性仍需验收。 | [会话模型](../crates/glim-core/src/session.rs)、[存储结构](../crates/glim-services/src/storage/schema.sql)、[草稿](../crates/glim-services/src/drafts.rs) |
| 图表 | Salt／Chen 已有常见语法适配，完整方言、预处理及外部资源兼容仍受限。 | [Salt](../scripts/markdown/salt.js)、[Chen](../scripts/markdown/chen.js) |
| 专业文件 | GPX／KML 已有坐标表，OBJ／STL 已有静态投影；GIS 地图、3D 交互及 GGUF／ONNX／NPZ 专用阅读器尚未提供。 | [地理表格](../crates/glim-services/src/geo.rs)、[静态投影](../crates/glim-services/src/vector_formats.rs)、[格式目录](../assets/format-catalog.json) |
| 安装 | 已有临时签名和 Sparkle 签名更新源；Developer ID 发布与 Apple 公证流程仍需证书、凭据及接入。 | [应用签名](../scripts/sign-macos.py)、[更新源](../scripts/create-update-feeds.py)、[发布流程](releasing.zh-CN.md) |

## 仍需覆盖的实机场景

- Apple Silicon／Intel 界面交互、图形环境和可访问性，多窗口与资源压力。
- 真实 GitHub 正式版本升级、需管理员权限的安装目录、多实例冲突、取消授权、替换中断及恢复；隔离安装回归不代替这些场景。
- 原生预览器的密码／坏文件、办公文档包、媒体编码、字体集合及可访问性。
- 活跃 SQLite／WAL 源、源变化拒绝、孤立 sidecar 及原库保持不变。
- Finder 冷／热启动打开、重复／跨窗口激活、关联更改／恢复及原生视图释放。
- 多窗口保存冲突、并发文件操作、监听刷新、大文件阅读器、草稿恢复及 macOS 手势。

## 当前范围外

非 macOS 平台、LSP、外部插件安装与动态加载不属于当前产品范围。既有公开附件继续保留；内部模块化遵循现有三层依赖与[模块契约](reader-modules.zh-CN.md)。

参见[架构](architecture.zh-CN.md)、[资源与测量](reader-resources.zh-CN.md)、[更新](auto-update-design.zh-CN.md)和[发布流程](releasing.zh-CN.md)。
