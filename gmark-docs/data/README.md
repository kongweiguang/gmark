<!-- @author kongweiguang -->

# JSON、JSONL、CSV、TSV 与 SVG

Gmark 不只渲染 Markdown。这个目录也放了几份很小的结构化数据和矢量图测试文件，方便观察 JSON Graph、记录导航、表格编辑和 SVG 预览。

## 文件清单

| 文件 | 打开后重点观察 |
| --- | --- |
| [JSON 样本](./json/sample.json) | JSON Graph、对象/数组折叠、标量编辑和源码定位 |
| [JSONL 样本](./json/sample.jsonl) | JSONL 记录结构、逐行导航和源码视图 |
| [CSV 样本](./table/sample.csv) | CSV 表格、筛选、单元格编辑和列结构 |
| [TSV 样本](./table/sample.tsv) | TSV 的制表符分隔和表格预览 |
| [SVG 样本](./svg/sample.svg) | SVG 源码、实时预览和分栏视图 |

这些文件都保持在几 KB 以内，只用于功能观察，不承担大文件压力测试。大文件和分页源码另有专门的性能测试，不放进这个面向用户的 Demo 目录。

JSON、JSONL、CSV 和 TSV 是机器格式，加入 Markdown 作者注释会改变数据本身；它们的维护说明和作者标识统一放在本页。SVG 则使用合法的 XML 注释保留作者标识。

## JSON Graph 观察点

打开 `json/sample.json` 后可以重点检查：

- 根对象、嵌套对象和数组是否能折叠、展开和聚焦。
- 字符串、数字、布尔值和 `null` 是否保持类型。
- 长数组、对象键名和中文文本在窄窗口中是否溢出。
- 从 Graph 编辑标量后，Source 是否仍然是有效 JSON。

## JSONL 观察点

打开 `json/sample.jsonl` 后可以检查每一行是否被当成独立记录，同时观察记录导航和源码定位。JSONL 的每一行都是完整 JSON 值，行间不使用逗号。

## CSV 与 TSV 观察点

`table/sample.csv` 和 `table/sample.tsv` 使用相同的数据字段，便于对比逗号和制表符分隔：

- 表头和数据行是否正确识别。
- 包含逗号、空格、中文和引号的单元格是否保持内容。
- Live 表格编辑后，Source 是否写回正确的分隔符。
- Preview 筛选和长文本换行是否稳定。

## SVG 观察点

`svg/sample.svg` 是一个自包含的小图，不引用外部字体、脚本或网络资源。可以在 Source、Preview 和 Split 之间切换，观察矢量图缩放是否清晰。

回到 [完整渲染总览](../markdown/00-rendering-showcase.md) 可以继续检查 Markdown 内容块。
