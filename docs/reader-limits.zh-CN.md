# 阅读器边界统一管理

所有应用层阅读边界以 [assets/reader-limits.json](../assets/reader-limits.json) 为唯一数值来源。修改对应分类的 `value`，保持 `type`、`unit` 和 `effect` 的含义一致，然后重新构建安装。空值表示已取消对应总量门槛；有限值用于具体功能、分页或资源预算，实际行为见说明。

构建脚本通过 `scripts/generate-reader-limits.py` 在 Cargo OUT_DIR 生成 Rust 常量、Swift ReaderLimits 和嵌入式 JSON；图表 WebView 接收同一份 JSON。运行时不扫描或读取配置文件。配置只管理阈值，解析格式是否合法、编解码支持及 UI 布局仍由对应实现负责。

本页由同一配置自动生成。修改配置后执行 `python3 scripts/generate-reader-limits.py --docs` 更新中英文清单；普通构建只生成 OUT_DIR 文件。

## openapi

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `openapi.field_bytes` | 4 KiB | bytes | 分页，可继续访问 | 每个索引 JSON 节点的标签／值解码前缀预算；完整原始 token 仍可通过原始值字节分页访问。 |
| `openapi.page_nodes` | 60 | items | 分页，可继续访问 | 接口或子节点的单页数量；导航替换当前页面，折叠内容释放 DOM。 |
| `openapi.depth` | 96 | levels | 降级处理 | 流式索引解析器的活动栈深度；更深的 JSON 保留普通源码阅读入口。 |

## text

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `text.editable_bytes` | 取消 | bytes | 无应用层总量门槛 | 内存允许时完整读取 UTF-8 文本，不因大小、行长或行数切换分页；内存不足时保留可编辑源码分页回退。 |
| `text.editable_newlines` | 取消 | items | 无应用层总量门槛 | 内存允许时完整读取 UTF-8 文本，不因大小、行长或行数切换分页；内存不足时保留可编辑源码分页回退。 |
| `text.line_bytes` | 取消 | bytes | 无应用层总量门槛 | 内存允许时完整读取 UTF-8 文本，不因大小、行长或行数切换分页；内存不足时保留可编辑源码分页回退。 |
| `text.page_bytes` | 1 MiB | bytes | 处理／内存预算 | 普通文本每页读取字节数，后续页面保留其余完整内容。 |
| `text.full_bytes` | 取消 | bytes | 无应用层总量门槛 | 内存允许时完整读取 UTF-8 文本，不因大小、行长或行数切换分页；内存不足时保留可编辑源码分页回退。 |
| `text.full_newlines` | 取消 | items | 无应用层总量门槛 | 内存允许时完整读取 UTF-8 文本，不因大小、行长或行数切换分页；内存不足时保留可编辑源码分页回退。 |
| `text.layout_run_bytes` | 1 KiB | bytes | 处理／内存预算 | 只读虚拟行单次排版的文本预算；超长逻辑行拆成后续显示行，源字节不丢失。 |

## binary

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `binary.preview_bytes` | 256 KiB | bytes | 分页，可继续访问 | 十六进制每页读取字节数，上一页／下一页可访问完整文件。 |

## image

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `image.input_bytes` | 取消 | bytes | 无应用层总量门槛 | 基于文件的原生图片读取不设输入字节或像素总量门槛，“文件字节”保留完整源文件访问。 |
| `image.pixels` | 取消 | pixels | 无应用层总量门槛 | 基于文件的原生图片读取不设输入字节或像素总量门槛，“文件字节”保留完整源文件访问。 |
| `image.decoded_bytes` | 512 MiB | bytes | 处理／内存预算 | 原生／QOI 当前预览图片的工作内存。QOI 流式采样像素，不拒绝较宽的源扫描行；原尺寸模式可能使用缩放图片，完整原始文件字节仍可访问。 |
| `image.preview_edge` | 4,000 | pixels | 处理／内存预算 | 初始 ImageIO 缩略图分辨率。原尺寸模式可在 image.decoded_bytes 内提高图片分辨率，不按原图尺寸拒绝文件。 |

## gif

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `gif.preview_edge` | 2,048 | pixels | 处理／内存预算 | 动画显示纹理边长，与源文件大小和总帧数无关。 |
| `gif.canvas_bytes` | 512 MiB | bytes | 处理／内存预算 | 合成工作内存预算；较大的源画布按行解码并采样，不拒绝整个动画。 |
| `gif.batch_bytes` | 16 MiB | bytes | 处理／内存预算 | 每批动画预取预算；已消费帧会释放，后续帧仍可继续读取。 |
| `gif.batch_frames` | 3 | items | 处理／内存预算 | 每批动画预取预算；已消费帧会释放，后续帧仍可继续读取。 |

## tiff

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `tiff.decode_buffer_bytes` | 取消 | bytes | 无应用层总量门槛 | ImageIO 按需读取所请求的 TIFF 页，不设应用层页数或累计预览总量门槛。 |
| `tiff.pages` | 取消 | items | 无应用层总量门槛 | ImageIO 按需读取所请求的 TIFF 页，不设应用层页数或累计预览总量门槛。 |
| `tiff.preview_decoded_bytes` | 取消 | bytes | 无应用层总量门槛 | ImageIO 按需读取所请求的 TIFF 页，不设应用层页数或累计预览总量门槛。 |
| `tiff.preview_encoded_bytes` | 取消 | bytes | 无应用层总量门槛 | ImageIO 按需读取所请求的 TIFF 页，不设应用层页数或累计预览总量门槛。 |

## inline_image

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `inline_image.preview_edge` | 2,048 | pixels | 处理／内存预算 | 内联缩略图的分辨率、解码及输出工作预算；原图仍可通过独立阅读器或源 URL 访问。 |
| `inline_image.encoded_bytes` | 取消 | bytes | 无应用层总量门槛 | 基于文件或流式准备图片，不设置应用编码文件总大小门槛。 |
| `inline_image.decoded_bytes` | 64 MiB | bytes | 处理／内存预算 | 内联缩略图的分辨率、解码及输出工作预算；原图仍可通过独立阅读器或源 URL 访问。 |
| `inline_image.output_overhead_bytes` | 1 MiB | bytes | 处理／内存预算 | 内联缩略图的分辨率、解码及输出工作预算；原图仍可通过独立阅读器或源 URL 访问。 |

## font

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `font.input_bytes` | 取消 | bytes | 无应用层总量门槛 | 系统基于文件读取字体元数据，不以应用总量门槛截掉字体面、资源或样本文本。 |
| `font.faces` | 取消 | items | 无应用层总量门槛 | 系统基于文件读取字体元数据，不以应用总量门槛截掉字体面、资源或样本文本。 |
| `font.resource_types` | 取消 | items | 无应用层总量门槛 | 系统基于文件读取字体元数据，不以应用总量门槛截掉字体面、资源或样本文本。 |
| `font.sample_chars` | 取消 | characters | 无应用层总量门槛 | 系统基于文件读取字体元数据，不以应用总量门槛截掉字体面、资源或样本文本。 |

## playlist

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `playlist.files` | 取消 | items | 无应用层总量门槛 | 虚拟化播放列表保留所有符合条件的文件，不设应用文件总数门槛。 |

## mkv

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `mkv.playlist_bytes` | 取消 | bytes | 无应用层总量门槛 | 从磁盘读取帧、用磁盘索引时间戳，并在可取消的辅助进程中为所选播放项写入碎片化 MP4；不设置总大小、总数量或总时长门槛。 |
| `mkv.input_bytes` | 取消 | bytes | 无应用层总量门槛 | 从磁盘读取帧、用磁盘索引时间戳，并在可取消的辅助进程中为所选播放项写入碎片化 MP4；不设置总大小、总数量或总时长门槛。 |
| `mkv.frame_bytes` | 16 MiB | bytes | 处理／内存预算 | 单个活动压缩帧／编解码元数据的分配预算，与影片总大小无关，原始文件字节仍可访问。 |
| `mkv.frames` | 取消 | items | 无应用层总量门槛 | 从磁盘读取帧、用磁盘索引时间戳，并在可取消的辅助进程中为所选播放项写入碎片化 MP4；不设置总大小、总数量或总时长门槛。 |
| `mkv.parser_read_bytes` | 2 GiB | bytes | 处理／内存预算 | 解析工作预算在每帧后重置，不累计限制影片总量；可选定位元数据通过流式扫描跳过。 |
| `mkv.parser_operations` | 20,000,000 | operations | 处理／内存预算 | 解析工作预算在每帧后重置，不累计限制影片总量；可选定位元数据通过流式扫描跳过。 |
| `mkv.tracks` | 取消 | items | 无应用层总量门槛 | 从磁盘读取帧、用磁盘索引时间戳，并在可取消的辅助进程中为所选播放项写入碎片化 MP4；不设置总大小、总数量或总时长门槛。 |
| `mkv.duration_seconds` | 取消 | seconds | 无应用层总量门槛 | 从磁盘读取帧、用磁盘索引时间戳，并在可取消的辅助进程中为所选播放项写入碎片化 MP4；不设置总大小、总数量或总时长门槛。 |
| `mkv.nal_units` | 取消 | items | 无应用层总量门槛 | 从磁盘读取帧、用磁盘索引时间戳，并在可取消的辅助进程中为所选播放项写入碎片化 MP4；不设置总大小、总数量或总时长门槛。 |
| `mkv.nesting` | 16 | levels | 处理／内存预算 | 轨道元数据解析栈预算；跳过可选章节、标签和定位索引，完整文件字节检查仍可用。 |
| `mkv.elements` | 取消 | items | 无应用层总量门槛 | 从磁盘读取帧、用磁盘索引时间戳，并在可取消的辅助进程中为所选播放项写入碎片化 MP4；不设置总大小、总数量或总时长门槛。 |

## json

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `json.input_bytes` | 取消 | bytes | 无应用层总量门槛 | 解析暂存与驻留节点均经内存准入后准备完整结构树，不设输入大小或节点总数门槛。 |
| `json.field_chars` | 512 | characters | 处理／内存预算 | 仅控制树标签摘要；展开容器或双击查看完整键／值，源码保留全部内容。 |
| `json.nodes` | 取消 | items | 无应用层总量门槛 | 解析暂存与驻留节点均经内存准入后准备完整结构树，不设输入大小或节点总数门槛。 |
| `json.description_chars` | 80 | characters | 处理／内存预算 | 仅控制树标签摘要；展开容器或双击查看完整键／值，源码保留全部内容。 |
| `json.array_summary_chars` | 120 | characters | 处理／内存预算 | 仅控制树标签摘要；展开容器或双击查看完整键／值，源码保留全部内容。 |
| `json.auto_expand_children` | 16 | items | 处理／内存预算 | 仅控制初始自动展开，折叠的子项仍可手动展开。 |

## csv

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `csv.input_bytes` | 取消 | bytes | 无应用层总量门槛 | 内存准入允许时准备完整行列；内存不足时发布完整行前缀并保留后续访问，不设固定行列配额。 |
| `csv.rows` | 取消 | items | 无应用层总量门槛 | 内存准入允许时准备完整行列；内存不足时发布完整行前缀并保留后续访问，不设固定行列配额。 |
| `csv.columns` | 取消 | items | 无应用层总量门槛 | 内存准入允许时准备完整行列；内存不足时发布完整行前缀并保留后续访问，不设固定行列配额。 |
| `csv.cells` | 取消 | items | 无应用层总量门槛 | 内存准入允许时准备完整行列；内存不足时发布完整行前缀并保留后续访问，不设固定行列配额。 |
| `csv.cell_chars` | 4,096 | characters | 处理／内存预算 | 可见单行单元格摘要；双击保留完整值，较大值使用分页查看。 |

## spreadsheet

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `spreadsheet.sheets` | 取消 | items | 无应用层总量门槛 | XLSX 单元格、工作表名称和已保存公式流式写入磁盘索引，值可分页查看；取消输入、工作表、行列、单元格和 HTML 总量门槛。旧版 XLS 及超出单条 XML 工作预算的情况使用无应用大小门槛的系统原生预览。 |
| `spreadsheet.rows` | 取消 | items | 无应用层总量门槛 | XLSX 单元格、工作表名称和已保存公式流式写入磁盘索引，值可分页查看；取消输入、工作表、行列、单元格和 HTML 总量门槛。旧版 XLS 及超出单条 XML 工作预算的情况使用无应用大小门槛的系统原生预览。 |
| `spreadsheet.columns` | 取消 | items | 无应用层总量门槛 | XLSX 单元格、工作表名称和已保存公式流式写入磁盘索引，值可分页查看；取消输入、工作表、行列、单元格和 HTML 总量门槛。旧版 XLS 及超出单条 XML 工作预算的情况使用无应用大小门槛的系统原生预览。 |
| `spreadsheet.cell_chars` | 取消 | characters | 无应用层总量门槛 | XLSX 单元格、工作表名称和已保存公式流式写入磁盘索引，值可分页查看；取消输入、工作表、行列、单元格和 HTML 总量门槛。旧版 XLS 及超出单条 XML 工作预算的情况使用无应用大小门槛的系统原生预览。 |
| `spreadsheet.input_bytes` | 取消 | bytes | 无应用层总量门槛 | XLSX 单元格、工作表名称和已保存公式流式写入磁盘索引，值可分页查看；取消输入、工作表、行列、单元格和 HTML 总量门槛。旧版 XLS 及超出单条 XML 工作预算的情况使用无应用大小门槛的系统原生预览。 |
| `spreadsheet.sheet_text_bytes` | 取消 | bytes | 无应用层总量门槛 | XLSX 单元格、工作表名称和已保存公式流式写入磁盘索引，值可分页查看；取消输入、工作表、行列、单元格和 HTML 总量门槛。旧版 XLS 及超出单条 XML 工作预算的情况使用无应用大小门槛的系统原生预览。 |
| `spreadsheet.html_bytes` | 取消 | bytes | 无应用层总量门槛 | XLSX 单元格、工作表名称和已保存公式流式写入磁盘索引，值可分页查看；取消输入、工作表、行列、单元格和 HTML 总量门槛。旧版 XLS 及超出单条 XML 工作预算的情况使用无应用大小门槛的系统原生预览。 |
| `spreadsheet.column_window` | 取消 | columns | 无应用层总量门槛 | 完整读取列范围或工作表目录；工作表行仍使用查询窗口及磁盘索引，投影单元格先进行内存准入。 |
| `spreadsheet.catalog_window` | 取消 | items | 无应用层总量门槛 | 完整读取列范围或工作表目录；工作表行仍使用查询窗口及磁盘索引，投影单元格先进行内存准入。 |

## xmind

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `xmind.title_bytes` | 取消 | bytes | 无应用层总量门槛 | 内存准入允许时准备完整思维导图内容；递归深度与格式合法性单独约束。 |
| `xmind.nesting` | 64 | levels | 降级处理 | 思维导图工作预算；超出（包括解析栈预算耗尽）后，以可取消的按需解压分页打开完整 content.json/content.xml。 |
| `xmind.content_bytes` | 取消 | bytes | 无应用层总量门槛 | 内存准入允许时准备完整思维导图内容；递归深度与格式合法性单独约束。 |
| `xmind.topics` | 取消 | items | 无应用层总量门槛 | 内存准入允许时准备完整思维导图内容；递归深度与格式合法性单独约束。 |
| `xmind.input_bytes` | 取消 | bytes | 无应用层总量门槛 | 从磁盘打开 ZIP 容器，不以文件总大小或条目总数拒绝打开。 |
| `xmind.archive_entries` | 取消 | items | 无应用层总量门槛 | 从磁盘打开 ZIP 容器，不以文件总大小或条目总数拒绝打开。 |
| `xmind.sheets` | 取消 | items | 无应用层总量门槛 | 内存准入允许时准备完整思维导图内容；递归深度与格式合法性单独约束。 |
| `xmind.relationships` | 取消 | items | 无应用层总量门槛 | 内存准入允许时准备完整思维导图内容；递归深度与格式合法性单独约束。 |
| `xmind.note_bytes` | 取消 | bytes | 无应用层总量门槛 | 内存准入允许时准备完整思维导图内容；递归深度与格式合法性单独约束。 |
| `xmind.labels` | 取消 | items | 无应用层总量门槛 | 内存准入允许时准备完整思维导图内容；递归深度与格式合法性单独约束。 |
| `xmind.xml_nodes` | 取消 | items | 无应用层总量门槛 | 内存准入允许时准备完整思维导图内容；递归深度与格式合法性单独约束。 |
| `xmind.xml_nesting` | 256 | levels | 降级处理 | 思维导图工作预算；超出（包括解析栈预算耗尽）后，以可取消的按需解压分页打开完整 content.json/content.xml。 |
| `xmind.link_bytes` | 取消 | bytes | 无应用层总量门槛 | 内存准入允许时准备完整思维导图内容；递归深度与格式合法性单独约束。 |
| `xmind.labels_bytes` | 取消 | bytes | 无应用层总量门槛 | 内存准入允许时准备完整思维导图内容；递归深度与格式合法性单独约束。 |

## archive

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `archive.entries` | 取消 | items | 无应用层总量门槛 | 枚举全部条目并落盘生成报告，界面按有界页面读取，不设置条目总数门槛。 |
| `archive.bytes` | 32 MiB | bytes | 处理／内存预算 | 结构化 XML 投影的工作缓冲预算，较大内容完整流式写入分页源码报告。 |

## numpy

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `numpy.header_bytes` | 64 KiB | bytes | 处理／内存预算 | 头部字符串 token 的工作缓冲预算；字段扫描覆盖完整流式头部，分页报告保留完整头部及支持类型的全部数值。 |
| `numpy.values` | 取消 | items | 无应用层总量门槛 | 支持类型的数值流式写入磁盘分页报告，不再只保留前缀数量。 |

## geometry

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `geometry.input_bytes` | 取消 | bytes | 无应用层总量门槛 | 内存准入后完整读取几何数据；栅格输出及格式合法性预算仍生效，并保留完整源码回退。 |
| `geometry.vertices` | 取消 | items | 无应用层总量门槛 | 内存准入后完整读取几何数据；栅格输出及格式合法性预算仍生效，并保留完整源码回退。 |
| `geometry.faces` | 取消 | items | 无应用层总量门槛 | 内存准入后完整读取几何数据；栅格输出及格式合法性预算仍生效，并保留完整源码回退。 |
| `geometry.polygon_vertices` | 取消 | items | 无应用层总量门槛 | 内存准入后完整读取几何数据；栅格输出及格式合法性预算仍生效，并保留完整源码回退。 |

## bdf

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `bdf.glyph_edge` | 64 | pixels | 降级处理 | 位图字体投影工作预算；超出后仍提供完整 BDF 源码分页，不静默缩短字形列表。 |
| `bdf.glyphs` | 64 | items | 降级处理 | 位图字体投影工作预算；超出后仍提供完整 BDF 源码分页，不静默缩短字形列表。 |

## executable

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `executable.input_bytes` | 取消 | bytes | 无应用层总量门槛 | 不设文件、名称、条目数或报告总量门槛。元数据读取使用 io.cache_bytes 工作缓存并落盘生成报告；缓存预算耗尽时打开完整文件字节分页。CString 不采用库内的前缀截取。 |
| `executable.name_chars` | 取消 | characters | 无应用层总量门槛 | 不设文件、名称、条目数或报告总量门槛。元数据读取使用 io.cache_bytes 工作缓存并落盘生成报告；缓存预算耗尽时打开完整文件字节分页。CString 不采用库内的前缀截取。 |
| `executable.architectures` | 取消 | items | 无应用层总量门槛 | 不设文件、名称、条目数或报告总量门槛。元数据读取使用 io.cache_bytes 工作缓存并落盘生成报告；缓存预算耗尽时打开完整文件字节分页。CString 不采用库内的前缀截取。 |
| `executable.hex_bytes` | 4 KiB | bytes | 处理／内存预算 | 元数据报告中的十六进制摘要，完整文件字节仍可通过原始字节分页访问。 |
| `executable.segments` | 取消 | items | 无应用层总量门槛 | 不设文件、名称、条目数或报告总量门槛。元数据读取使用 io.cache_bytes 工作缓存并落盘生成报告；缓存预算耗尽时打开完整文件字节分页。CString 不采用库内的前缀截取。 |
| `executable.sections` | 取消 | items | 无应用层总量门槛 | 不设文件、名称、条目数或报告总量门槛。元数据读取使用 io.cache_bytes 工作缓存并落盘生成报告；缓存预算耗尽时打开完整文件字节分页。CString 不采用库内的前缀截取。 |
| `executable.report_bytes` | 取消 | bytes | 无应用层总量门槛 | 不设文件、名称、条目数或报告总量门槛。元数据读取使用 io.cache_bytes 工作缓存并落盘生成报告；缓存预算耗尽时打开完整文件字节分页。CString 不采用库内的前缀截取。 |
| `executable.libraries` | 取消 | items | 无应用层总量门槛 | 不设文件、名称、条目数或报告总量门槛。元数据读取使用 io.cache_bytes 工作缓存并落盘生成报告；缓存预算耗尽时打开完整文件字节分页。CString 不采用库内的前缀截取。 |
| `executable.dynamic_entries` | 取消 | items | 无应用层总量门槛 | 不设文件、名称、条目数或报告总量门槛。元数据读取使用 io.cache_bytes 工作缓存并落盘生成报告；缓存预算耗尽时打开完整文件字节分页。CString 不采用库内的前缀截取。 |
| `executable.name_scan_bytes` | 取消 | bytes | 无应用层总量门槛 | 不设文件、名称、条目数或报告总量门槛。元数据读取使用 io.cache_bytes 工作缓存并落盘生成报告；缓存预算耗尽时打开完整文件字节分页。CString 不采用库内的前缀截取。 |

## tensor

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `tensor.header_bytes` | 取消 | bytes | 无应用层总量门槛 | 元数据记录及 dtype 计数流式落盘；过大的单条记录改为完整头部分页，不读取张量权重。不设置头部、名称、形状、条目数量或报告总量门槛。 |
| `tensor.rows` | 取消 | items | 无应用层总量门槛 | 元数据记录及 dtype 计数流式落盘；过大的单条记录改为完整头部分页，不读取张量权重。不设置头部、名称、形状、条目数量或报告总量门槛。 |
| `tensor.report_bytes` | 取消 | bytes | 无应用层总量门槛 | 元数据记录及 dtype 计数流式落盘；过大的单条记录改为完整头部分页，不读取张量权重。不设置头部、名称、形状、条目数量或报告总量门槛。 |
| `tensor.shape_dimensions` | 取消 | items | 无应用层总量门槛 | 元数据记录及 dtype 计数流式落盘；过大的单条记录改为完整头部分页，不读取张量权重。不设置头部、名称、形状、条目数量或报告总量门槛。 |
| `tensor.metadata_entries` | 取消 | items | 无应用层总量门槛 | 元数据记录及 dtype 计数流式落盘；过大的单条记录改为完整头部分页，不读取张量权重。不设置头部、名称、形状、条目数量或报告总量门槛。 |
| `tensor.name_chars` | 取消 | characters | 无应用层总量门槛 | 元数据记录及 dtype 计数流式落盘；过大的单条记录改为完整头部分页，不读取张量权重。不设置头部、名称、形状、条目数量或报告总量门槛。 |
| `tensor.dtype_chars` | 取消 | characters | 无应用层总量门槛 | 元数据记录及 dtype 计数流式落盘；过大的单条记录改为完整头部分页，不读取张量权重。不设置头部、名称、形状、条目数量或报告总量门槛。 |
| `tensor.metadata_key_chars` | 取消 | characters | 无应用层总量门槛 | 元数据记录及 dtype 计数流式落盘；过大的单条记录改为完整头部分页，不读取张量权重。不设置头部、名称、形状、条目数量或报告总量门槛。 |
| `tensor.metadata_value_chars` | 取消 | characters | 无应用层总量门槛 | 元数据记录及 dtype 计数流式落盘；过大的单条记录改为完整头部分页，不读取张量权重。不设置头部、名称、形状、条目数量或报告总量门槛。 |

## sqlite

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `sqlite.snapshot_bytes` | 取消 | bytes | 无应用层总量门槛 | 按固定大小缓冲块复制快照，不设置应用数据库总大小门槛。 |
| `sqlite.page_rows` | 500 | rows | 分页，可继续访问 | 表格／结构每页缓冲行数，与表总行数无关；常驻分页栏提供行数统计、页码跳转及首页／末页导航。 |
| `sqlite.tables` | 取消 | items | 无应用层总量门槛 | 列出全部表和视图，不设置应用目录条目上限。 |
| `sqlite.columns` | 取消 | columns | 无应用层总量门槛 | 所有 SQLite 列均可访问；通过横向虚拟化限制工作集。 |
| `sqlite.value_bytes` | 取消 | bytes | 无应用层总量门槛 | 沿用原生 SQLite 格式约束；完整值及定义按 text.page_bytes 的窗口读取，并提供上一页／下一页。 |
| `sqlite.text_bytes` | 512 | bytes | 处理／内存预算 | 仅控制单元格摘要；双击打开完整值分页，二进制值以十六进制显示。 |
| `sqlite.schema_text_bytes` | 取消 | bytes | 无应用层总量门槛 | 沿用原生 SQLite 格式约束；完整值及定义按 text.page_bytes 的窗口读取，并提供上一页／下一页。 |
| `sqlite.blob_bytes` | 16 | bytes | 处理／内存预算 | 仅控制单元格摘要；双击打开完整值分页，二进制值以十六进制显示。 |
| `sqlite.query_seconds` | 5 | seconds | 超时 | 初始可取消查询时间预算，“延长时间重试”可明确增加预算。 |
| `sqlite.busy_milliseconds` | 1,000 | milliseconds | 超时 | 原生 SQLite 锁等待时间；数据库忙碌时可重试，与文件大小无关。 |
| `sqlite.cache_kib` | 8,192 | KiB | 处理／内存预算 | SQLite 页缓存工作内存，较大的数据库继续从磁盘读取页面。 |
| `sqlite.column_window` | 32 | columns | 分页，可继续访问 | 横向视口每次读取的列数，不限制总列数。 |

## markup

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `markup.eager_layout_bytes` | 取消 | bytes | 无应用层总量门槛 | Markdown／标记预览完整测量布局，不因字节数或行数切换为估算高度；图片与图表保留按需解码。 |
| `markup.eager_layout_lines` | 取消 | source lines | 无应用层总量门槛 | Markdown／标记预览完整测量布局，不因字节数或行数切换为估算高度；图片与图表保留按需解码。 |
| `markup.scan_bytes` | 取消 | bytes | 无应用层总量门槛 | 扫描当前文本中的图片引用；大文档使用源码分页，不以引用扫描总量截掉后半部分。 |

## remote_image

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `remote_image.input_bytes` | 取消 | bytes | 无应用层总量门槛 | HTTP 下载流式写入临时文件，再生成有界缩略图，不以源输入总大小拒绝文件。 |
| `remote_image.timeout_seconds` | 20 | seconds | 超时 | 空闲／无进度超时，持续有进度的下载不设固定总耗时上限。 |
| `remote_image.first_attempt_seconds` | 8 | seconds | 超时 | 连接建立时间预算，包括 GitHub 原始 URL 首次尝试后转用 API 的连接等待。 |
| `remote_image.svg_embedded_bytes` | 64 MiB | bytes | 处理／内存预算 | SVG 准备过程中单个内嵌图片的工作内存，原始 SVG 内容仍可通过源码或文件字节访问。 |
| `remote_image.redirects` | 5 | items | 处理／内存预算 | 重定向链预算用于避免请求循环，不限制图片文件总大小。 |
| `remote_image.connect_seconds` | 8 | seconds | 超时 | 连接建立时间预算，包括 GitHub 原始 URL 首次尝试后转用 API 的连接等待。 |
| `remote_image.minimum_bytes_per_second` | 1 | bytes/second | 超时 | 空闲超时使用的最低进度判定，不是带宽上限。 |

## diagram

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `diagram.disk_cache_bytes` | 512 MiB | bytes | 处理／内存预算 | 图表结果磁盘缓存容量；淘汰最久未使用的结果。未命中、条目损坏或存储不可用时，沿用正常后台路径重新生成图表。 |
| `diagram.blocks` | 8 | items | 处理／内存预算 | 后台每批图表渲染工作预算；可见图块自动排队，不限制文档公式总数，也无需手动翻页。 |
| `diagram.source_bytes` | 256 KiB | bytes | 降级处理 | 单个图表的结构化渲染工作预算；失败时保留完整图块源码，其余图块仍可按需访问。 |
| `diagram.svg_bytes` | 8 MiB | bytes | 降级处理 | 单个图表的结构化渲染工作预算；失败时保留完整图块源码，其余图块仍可按需访问。 |
| `diagram.aggregate_encoded_bytes` | 64 MiB | bytes | 处理／内存预算 | 可视区域图表缓存与当前渲染批次的编码／解码工作预算；旧资源可淘汰后再次按需生成，不限制文档图块总数。 |
| `diagram.aggregate_decoded_bytes` | 128 MiB | bytes | 处理／内存预算 | 可视区域图表缓存与当前渲染批次的编码／解码工作预算；旧资源可淘汰后再次按需生成，不限制文档图块总数。 |
| `diagram.preview_edge` | 2,400 | pixels | 处理／内存预算 | 当前图片的分辨率／缩放预算；每页图片尺寸还按聚合工作内存调整，与文档总长度无关。 |
| `diagram.startup_seconds` | 15 | seconds | 超时 | 可取消的渲染器启动／单块时间预算；未完成图块显示完整源码，不拒绝整个 Markdown 文档。 |
| `diagram.block_seconds` | 8 | seconds | 超时 | 可取消的渲染器启动／单块时间预算；未完成图块显示完整源码，不拒绝整个 Markdown 文档。 |
| `diagram.ipc_bytes` | 128 MiB | bytes | 处理／内存预算 | 一页请求或单图块的 IPC 工作缓冲预算；文档按页处理，过大的渲染结果保留完整图块源码。 |
| `diagram.slot_bytes` | 16 MiB | bytes | 处理／内存预算 | 一页请求或单图块的 IPC 工作缓冲预算；文档按页处理，过大的渲染结果保留完整图块源码。 |
| `diagram.max_upscale` | 2 | scale | 处理／内存预算 | 当前图片的分辨率／缩放预算；每页图片尺寸还按聚合工作内存调整，与文档总长度无关。 |
| `diagram.request_bytes` | 64 MiB | bytes | 处理／内存预算 | 一页请求或单图块的 IPC 工作缓冲预算；文档按页处理，过大的渲染结果保留完整图块源码。 |
| `diagram.message_bytes` | 16388 KiB | bytes | 处理／内存预算 | 一页请求或单图块的 IPC 工作缓冲预算；文档按页处理，过大的渲染结果保留完整图块源码。 |
| `diagram.mermaid_edges` | 1,000 | edges | 降级处理 | 单个图表的结构化渲染工作预算；失败时保留完整图块源码，其余图块仍可按需访问。 |
| `diagram.math_macros` | 1,000 | expansions | 处理／内存预算 | 公式解释器的展开工作预算，控制递归工作量；无法完成渲染时仍保留完整公式源码。 |

## book

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `book.chapter_bytes` | 取消 | bytes | 无应用层总量门槛 | 内存允许时完整读取所选章节、文本或 XML；章节导航、嵌套、内嵌图片及输出预算单独管理。 |
| `book.xml_nodes` | 取消 | nodes | 无应用层总量门槛 | 内存允许时完整读取所选章节、文本或 XML；章节导航、嵌套、内嵌图片及输出预算单独管理。 |
| `book.resource_bytes` | 取消 | bytes | 无应用层总量门槛 | 所请求的包内图片流式写入私有临时文件，再由原生接口生成缩略图；不按资源字节总量或原图边长拒绝打开，仅保留当前章节／页面预览。 |
| `book.preview_edge` | 2,400 | pixels | 处理／内存预算 | 当前插图／漫画页的缩略图分辨率，不以此拒绝原始图片。 |
| `book.input_bytes` | 取消 | bytes | 无应用层总量门槛 | 取消书籍、压缩包条目、章节、目录和元数据总量门槛。结构化解析保留工作预算；复杂元数据改为列出全部包内成员，提供完整源码或字节分页。 |
| `book.text_bytes` | 取消 | bytes | 无应用层总量门槛 | 内存允许时完整读取所选章节、文本或 XML；章节导航、嵌套、内嵌图片及输出预算单独管理。 |
| `book.archive_entries` | 取消 | items | 无应用层总量门槛 | 取消书籍、压缩包条目、章节、目录和元数据总量门槛。结构化解析保留工作预算；复杂元数据改为列出全部包内成员，提供完整源码或字节分页。 |
| `book.chapters` | 取消 | items | 无应用层总量门槛 | 取消书籍、压缩包条目、章节、目录和元数据总量门槛。结构化解析保留工作预算；复杂元数据改为列出全部包内成员，提供完整源码或字节分页。 |
| `book.metadata_bytes` | 取消 | bytes | 无应用层总量门槛 | 取消书籍、压缩包条目、章节、目录和元数据总量门槛。结构化解析保留工作预算；复杂元数据改为列出全部包内成员，提供完整源码或字节分页。 |
| `book.toc_entries` | 取消 | items | 无应用层总量门槛 | 取消书籍、压缩包条目、章节、目录和元数据总量门槛。结构化解析保留工作预算；复杂元数据改为列出全部包内成员，提供完整源码或字节分页。 |
| `book.toc_depth` | 32 | levels | 处理／内存预算 | 仅控制目录的视觉缩进，保留全部嵌套条目及实际层级，不截掉目录。 |
| `book.fb2_depth` | 64 | levels | 降级处理 | 章节结构化排版工作预算。超出时保留章节导航并提供完整源码分页，支持的旧编码通过流式转码读取，不静默截掉章节。 |
| `book.html_depth` | 128 | levels | 降级处理 | 章节结构化排版工作预算。超出时保留章节导航并提供完整源码分页，支持的旧编码通过流式转码读取，不静默截掉章节。 |
| `book.chapter_decoded_bytes` | 128 MiB | bytes | 降级处理 | 章节结构化排版工作预算。超出时保留章节导航并提供完整源码分页，支持的旧编码通过流式转码读取，不静默截掉章节。 |
| `book.chapter_images` | 取消 | items | 无应用层总量门槛 | 内存允许时完整读取所选章节、文本或 XML；章节导航、嵌套、内嵌图片及输出预算单独管理。 |
| `book.image_decoded_bytes` | 取消 | bytes | 无应用层总量门槛 | 所请求的包内图片流式写入私有临时文件，再由原生接口生成缩略图；不按资源字节总量或原图边长拒绝打开，仅保留当前章节／页面预览。 |
| `book.image_edge` | 取消 | pixels | 无应用层总量门槛 | 所请求的包内图片流式写入私有临时文件，再由原生接口生成缩略图；不按资源字节总量或原图边长拒绝打开，仅保留当前章节／页面预览。 |

## book_state

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `book_state.bookmarks` | 取消 | items | 无应用层总量门槛 | 保存全部书签、标签和阅读状态记录，不设置应用书签总数或输入总大小门槛。 |
| `book_state.label_bytes` | 取消 | bytes | 无应用层总量门槛 | 保存全部书签、标签和阅读状态记录，不设置应用书签总数或输入总大小门槛。 |
| `book_state.input_bytes` | 取消 | bytes | 无应用层总量门槛 | 保存全部书签、标签和阅读状态记录，不设置应用书签总数或输入总大小门槛。 |
| `book_state.label_chars` | 120 | characters | 处理／内存预算 | 派生书签、书籍标题和目录标签摘要；章节目标及完整元数据／文本弹窗保留全部内容。 |

## geo

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `geo.points` | 取消 | items | 无应用层总量门槛 | 内存允许时准备全部坐标行；内存不足时可发布完整前缀并保留后续访问。 |

## workspace

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `workspace.content_buffer_bytes` | 64 KiB | bytes | 处理／内存预算 | 内容搜索流式读取缓冲，不限制文件或单行总大小。 |
| `workspace.content_snippet_bytes` | 320 | bytes per match | 处理／内存预算 | 每条命中保留的预览字节预算；搜索仍检查完整行与完整文件。 |
| `workspace.search_debounce_ms` | 150 | milliseconds | 处理／内存预算 | 仅搜索输入变化使用的防抖间隔；打开搜索、切换模式与翻页立即执行。 |
| `workspace.search_results` | 60 | items | 分页，可继续访问 | 可见页／批次大小，后续搜索结果或已保存工作区可通过继续浏览入口访问。 |
| `workspace.search_entries` | 取消 | items | 无应用层总量门槛 | 搜索遍历全部符合条件的条目并提供可取消的结果分页，不设置扫描条目总数截断。 |
| `workspace.recent_workspaces` | 30 | items | 分页，可继续访问 | 可见页／批次大小，后续搜索结果或已保存工作区可通过继续浏览入口访问。 |

## update

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `update.session_bytes` | 取消 | bytes | 无应用层总量门槛 | 流式读写会话 JSON，校验身份及活动索引，保留全部实际窗口、根目录和标签页。 |
| `update.workspaces` | 取消 | items | 无应用层总量门槛 | 流式读写会话 JSON，校验身份及活动索引，保留全部实际窗口、根目录和标签页。 |
| `update.roots_per_workspace` | 取消 | items | 无应用层总量门槛 | 流式读写会话 JSON，校验身份及活动索引，保留全部实际窗口、根目录和标签页。 |
| `update.tabs_per_workspace` | 取消 | items | 无应用层总量门槛 | 流式读写会话 JSON，校验身份及活动索引，保留全部实际窗口、根目录和标签页。 |

## audio_artwork

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `audio_artwork.input_bytes` | 取消 | bytes | 无应用层总量门槛 | 原生元数据封面不设置应用层字节门槛；“打开完整封面”提供完整原始快照。 |
| `audio_artwork.preview_edge` | 600 | pixels | 处理／内存预算 | 当前封面缩略图分辨率；“打开完整封面”保留完整原图访问入口。 |

## io

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `io.buffer_bytes` | 64 KiB | bytes | 处理／内存预算 | 可取消流式读取、复制及报告生成的固定工作缓冲，与源文件总大小无关。 |
| `io.cache_bytes` | 64 MiB | bytes | 处理／内存预算 | 解码文本、可执行文件元数据及网格／字体投影缓冲的工作缓存预算。旧文本范围可重新生成；较大的表示仍保留完整源码／字节分页。 |
| `io.record_bytes` | 64 MiB | bytes | 处理／内存预算 | 单条活动 XML／结构化元数据记录或值的预算，与文件总大小无关；过大的记录仍可通过完整源码／头部分页或原生预览访问。 |

## svg

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `svg.event_bytes` | 64 MiB | bytes | 处理／内存预算 | 清理器中单条活动 XML 标记／文本记录的工作预算，不是文档总量门槛；超出准备预算时，原生阅读器“文件字节”仍保留完整源文件。 |

## word

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `word.zoom_min_percent` | 25 | percent | 处理／内存预算 | Word 阅读视图最小显示缩放，不约束源文件内容。 |
| `word.zoom_max_percent` | 400 | percent | 处理／内存预算 | Word 阅读视图最大显示缩放，不约束源文件内容。 |
| `word.zoom_step_percent` | 25 | percent | 处理／内存预算 | Word 阅读视图缩放按钮的百分比步长；重置恢复 100%。 |

## drawio

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `drawio.xml_bytes` | 取消 | bytes | 无应用层总量门槛 | 内存准入后完整读取源码及解压页 XML；栅格输出与嵌套预算单独约束。 |
| `drawio.nesting` | 64 | levels | 处理／内存预算 | 限制父级遍历深度并检测循环分组。 |
| `drawio.preview_bytes` | 128 MiB | bytes | 处理／内存预算 | 所有 PNG 预览页的总编码内存预算；超过时回退到源码分页。 |

## opml

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `opml.xml_bytes` | 取消 | bytes | 无应用层总量门槛 | 解析内存准入后准备完整大纲；嵌套深度与格式合法性单独约束。 |
| `opml.nodes` | 取消 | items | 无应用层总量门槛 | 解析内存准入后准备完整大纲；嵌套深度与格式合法性单独约束。 |
| `opml.nesting` | 256 | levels | 处理／内存预算 | 大纲嵌套深度预算。 |

## watch

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `watch.pending_paths` | 4,096 | paths | 降级处理 | 待处理文件变化路径的去重预算。超出后执行完整来源检查，不丢弃变化，也不限制文档访问。 |

## jobs

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `jobs.content_search_concurrency` | 1 | jobs | 处理／内存预算 | 按需内容搜索并发预算，独立于文件名索引与文件打开。 |
| `jobs.document_concurrency` | 2 | jobs | 处理／内存预算 | 进程内文档加载与刷新的并发任务预算。其余任务异步排队并可取消，不限制文件访问。 |
| `jobs.projection_concurrency` | 2 | jobs | 处理／内存预算 | 进程内数据投影与分页准备的并发任务预算。其余任务异步排队并可取消，不限制文件访问。 |
| `jobs.decode_concurrency` | 2 | jobs | 处理／内存预算 | 进程内图片与动画解码的并发任务预算。其余任务异步排队并可取消，不限制文件访问。 |
| `jobs.native_concurrency` | 2 | jobs | 处理／内存预算 | 进程内原生文档准备的并发任务预算。其余任务异步排队并可取消，不限制文件访问。 |
| `jobs.diagram_concurrency` | 1 | jobs | 处理／内存预算 | 进程内图表批次准备的并发预算。排队批次不占工作线程，并保留取消标志。 |

## resources

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `resources.cache_bytes` | 256 MiB | bytes | 处理／内存预算 | 进程级可回收阅读器缓存预算；先回收后台缓存，再回收最近最少使用且无使用者持有的值。存活状态与活动工作另行记账，不作为缓存丢弃；框架内部内存不在此账本内。 |
| `resources.working_bytes` | 取消 | bytes | 无应用层总量门槛 | 工作分配使用 macOS 实时内存余量，不设固定字节预算。 |
| `resources.system_used_percent` | 95 | percent | 处理／内存预算 | 系统内存占用的紧急停止线，剩余物理内存留给 macOS 和其他应用。 |
| `resources.memory_poll_ms` | 250 | milliseconds | 处理／内存预算 | 系统内存准入的后台采样间隔。 |

## remote

| Key | 当前值 | 单位 | 超限效果 | 实际行为 |
| --- | ---: | --- | --- | --- |
| `remote.connection_check_interval_ms` | 30,000 | milliseconds | 处理／内存预算 | 后台 SFTP 连接检查成功后的周期；应用启动即尝试所有已保存连接，无需激活工作区。 |
| `remote.connection_retry_ms` | 5,000 | milliseconds | 处理／内存预算 | 连接失败后的首次静默重试间隔；逐次翻倍，上限为 connection_retry_max_ms，后台尝试不弹出认证对话框。 |
| `remote.connection_retry_max_ms` | 60,000 | milliseconds | 处理／内存预算 | 后台连接失败重试的最大间隔；连接仍被保存且应用运行时持续重试。 |
| `remote.connection_concurrency` | 2 | connections | 处理／内存预算 | 后台连接检查或重连并发预算；其余已保存连接排队，不阻塞界面。 |
| `remote.search_retry_initial_ms` | 10,000 | milliseconds | 处理／内存预算 | 索引连接或目录失败后的首次静默重试间隔；逐次翻倍，上限为 search_refresh_ms。 |
| `remote.search_concurrency` | 2 | jobs | 处理／内存预算 | 远端路径索引任务并发预算，独立于内容搜索和文件打开；排队任务不占工作线程。 |
| `remote.search_batch_paths` | 256 | paths per transaction | 处理／内存预算 | 每个搜索索引事务提交的路径数；首个文件或达到发布间隔时提前发布部分结果，不限制总路径数。 |
| `remote.search_poll_ms` | 250 | milliseconds | 处理／内存预算 | 搜索索引结果发布和界面版本轮询间隔；版本不变时不重新执行文件名查询。 |
| `remote.search_directory_batch` | 64 | directories per cycle | 处理／内存预算 | 单个搜索索引增量任务的目录元数据检查预算；变化目录的列表通过磁盘暂存流式处理，不限制目录总数。 |
| `remote.search_directory_poll_ms` | 2,000 | milliseconds | 处理／内存预算 | 每个驻留远端工作区的后台增量目录批次间隔，不依赖搜索弹窗。 |
| `remote.search_refresh_ms` | 300,000 | milliseconds | 处理／内存预算 | 目录列表达到此核对年龄后，即使粗粒度修改时间未变也重新核对；同时作为指数重试间隔上限。缓存路径不因时间过期，手动重试唤醒失败目录，不重建完整索引。 |
| `remote.search_record_bytes` | 1 MiB | bytes | 降级处理 | SSH 文件名流中单个 NUL 分隔记录的活动预算；无效或超预算命令输出降级到流式 SFTP 枚举，不限制文件或工作区总大小。 |
| `remote.refresh_interval_ms` | 2,000 | milliseconds | 处理／内存预算 | 当前可见远端工作区自动刷新周期的间隔；应用未激活或传输忙碌、断开时暂停。 |
| `remote.refresh_batch` | 4 | paths per cycle | 处理／内存预算 | 每轮轮换检查的目录和后台文件数；当前文件优先检查。限制活动工作范围，不限制工作区总大小。 |
| `remote.idle_timeout_ms` | 30,000 | milliseconds | 超时 | SFTP 请求等待收发数据时，连续无字节进展达到此时长则关闭连接。收发进展重置计时，不限制文件总大小或传输总时长。 |
| `remote.packet_bytes` | 1 MiB | bytes | 处理／内存预算 | 单个活动 SFTP 协议包的预算；文件通过连续分块传输，不限制总大小。 |
| `remote.transfer_bytes` | 32 KiB | bytes | 处理／内存预算 | 每次 SFTP 读取请求的字节数，与远端文件总大小无关。 |
| `remote.diagnostic_bytes` | 16 KiB | bytes | 处理／内存预算 | 为连接失败保留的 SSH 诊断尾部预算，更早的输出会被丢弃。 |
