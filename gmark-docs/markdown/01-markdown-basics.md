<!-- @author kongweiguang -->

# 基础 Markdown 与组合排版

本页集中展示从参考文档整理出的通用 Markdown 写法。所有内容都可以直接保存为普通 `.md` 文件，不需要额外的字段定义。

## 段落、引用和分隔线

连续文字组成段落。段落之间保留空行可以让结构更清楚，也便于 GMark 的 Live 视图逐块编辑。

---

引用块可以有多个段落：

> 第一段引用内容，包含 **强调** 和 `代码`。
>
> 第二段引用内容可以继续说明背景。

## 列表与任务清单

- 一级项目
  - 二级项目
    - 三级项目
- 另一个项目

1. 需求梳理
2. 内容编写
3. 预览校对

任务清单可以和普通列表混合：

- [x] 读取参考文档
- [x] 清理专有字段
- [ ] 在浅色主题中检查颜色
- [ ] 在深色主题中检查对比度
  - [x] 检查嵌套任务
  - [ ] 检查长文本换行

## 表格与行内元素

| 写法 | 源码示例 | 说明 |
| --- | --- | --- |
| 粗体 | **重点** | 语义强调 |
| 斜体 | *备注* | 轻量强调 |
| 删除线 | ~~旧版本~~ | 表示已移除内容 |
| 下划线 | <u>术语</u> | 使用安全 HTML |
| 上下标 | x^2^、H~2~O | 数学和化学记号 |

表格单元格也可以包含 [链接](https://example.com)、`代码` 和 $x + y$。

## 链接引用和图片

普通链接适合跨文档导航：[代码示例](./02-code.md)、[数学公式](./03-math.md)、[Mermaid 图表](./04-mermaid.md)。

也可以把引用链接集中放在文档末尾：[项目仓库][gmark-repo] 和 [本地图片][gmark-icon]。

![GMark 图标](../../assets/icon/gmark-icon-128.png "本地图片")

[gmark-repo]: https://github.com/kongweiguang/gmark "GMark repository"
[gmark-icon]: ../../assets/icon/gmark-icon-64.png "GMark icon"

## Callout 与嵌套块

> [!NOTE]
> Callout 本质上仍然是引用块，可以包含列表和行内格式。
>
> - 一条说明
> - 另一条 **重点**

> [!WARNING]
> 长文本会随着窗口宽度换行。建议把复杂解释放在正文，而不是塞进标题或节点里。

> [!TIP]
> 使用 Preview 观察阅读效果，使用 Source 检查空格、缩进和围栏。

## 脚注

脚注引用不会打断主段落[^markdown]，也可以连续写多个引用[^gmark][^gfm]。

脚注定义支持行内格式：

[^markdown]: Markdown 是一种轻量标记语言，强调源文件可读性。
[^gmark]: GMark 保留源文件，同时提供 Live、Source、Split 和 Preview 视图。
[^gfm]: GFM 扩展包含表格、任务列表、删除线和 Callout 等常见能力。

## 安全 HTML

GMark 会对 HTML 做安全清理，只保留受支持的语义标签和安全属性。

<details open>
<summary>展开查看 HTML 内容</summary>

<div style="border: 1px solid #cbd5e1; padding: 12px; border-radius: 8px;">
  <p><strong>安全 HTML</strong> 可以补充 Markdown 不方便表达的小结构。</p>
  <p><mark>危险脚本、iframe、video 和 audio 不会被执行。</mark></p>
</div>

</details>

## 混合排版

> [!IMPORTANT]
> 一个完整的文档片段可以同时包含段落、任务、表格、代码和公式。
>
> ```text
> status = "draft"
> ```
>
> | 字段 | 值 |
> | --- | --- |
> | 公式 | $f(x) = x^2$ |
> | 任务 | - [x] 已完成 |

下一页：[代码示例](./02-code.md)。
