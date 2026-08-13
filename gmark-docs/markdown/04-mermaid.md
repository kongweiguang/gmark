<!-- @author kongweiguang -->

# Mermaid 图表

Mermaid 图表使用 `mermaid` 代码块。下面的示例来自常见的流程、系统设计和知识整理场景，适合在 Gmark 中分别查看源码、预览和分屏效果。

## 流程图

```mermaid
flowchart TD
    A[打开文档] --> B[读取 Markdown]
    B --> C{解析成功?}
    C -->|是| D[构建渲染块]
    C -->|否| E[保留源码]
    D --> F[显示 Live / Preview]
```

带分组的系统协作流程：

```mermaid
flowchart LR
    subgraph 编辑器
        A[输入内容] --> B[保存文件]
    end

    subgraph 文档引擎
        C[解析] --> D[生成块]
    end

    subgraph 导出
        E[HTML] --> F[PNG / PDF]
    end

    B --> C
    D --> E
```

## 时序图

```mermaid
sequenceDiagram
    participant U as 用户
    participant E as 编辑器
    participant P as 解析器
    participant R as 渲染器

    U->>E: 打开 Markdown
    E->>P: 发送源码
    P-->>E: 返回块与行内节点
    E->>R: 请求当前主题渲染
    R-->>E: 返回视图内容
    E-->>U: 显示结果
```

## 状态图

```mermaid
stateDiagram-v2
    [*] --> 草稿
    草稿 --> 编辑中: 开始修改
    编辑中 --> 已保存: 保存成功
    编辑中 --> 草稿: 放弃修改
    已保存 --> 已导出: 导出完成
    已导出 --> [*]
```

## 类图

```mermaid
classDiagram
    class Document {
        +String source
        +parse()
        +to_markdown()
    }

    class Block {
        +String kind
        +render()
    }

    class Renderer {
        +render_markdown()
        +render_mermaid()
    }

    Document --> Block : contains
    Renderer --> Document : reads
```

## ER 图

```mermaid
erDiagram
    WORKSPACE ||--o{ DOCUMENT : contains
    DOCUMENT ||--o{ ASSET : references
    DOCUMENT {
        string id
        string path
        string title
    }
    ASSET {
        string id
        string path
        string kind
    }
```

## 甘特图

```mermaid
gantt
    title Gmark Demo 整理计划
    dateFormat YYYY-MM-DD

    section 内容
    参考资料整理 :done, docs1, 2026-08-01, 2d
    示例编写     :active, docs2, after docs1, 3d

    section 校验
    源码检查     :check1, after docs2, 1d
    主题检查     :check2, after check1, 1d
```

## 饼图

```mermaid
pie title Demo 内容分布
    "Markdown" : 35
    "代码" : 25
    "数学" : 20
    "Mermaid" : 20
```

## Git 图

```mermaid
gitGraph
    commit id: "init"
    branch feature/docs
    checkout feature/docs
    commit id: "add-showcase"
    commit id: "add-math"
    checkout main
    merge feature/docs
    commit id: "release"
```

## Mindmap

```mermaid
mindmap
  root((Gmark))
    文档
      Markdown
      代码
      数学
    视图
      Live
      Source
      Split
      Preview
    导出
      HTML
      PNG
      PDF
```

## 用户旅程图

```mermaid
journey
    title 第一次查看渲染示例
    section 打开
      选择 gmark-docs: 5: 读者
      打开总览: 5: 读者
    section 检查
      切换 Preview: 4: 读者
      查看代码和公式: 5: 读者
      调整窗口宽度: 3: 读者
```

## 图表书写建议

- 一张图只表达一个主题，节点文字尽量短。
- 流程图优先使用 `TD` 或 `LR`，避免一次放入过多节点。
- 时序图把一次消息写成一个动作；复杂说明放在正文。
- Mermaid 出现错误时，先用最小图确认代码块标记和图表类型，再逐步增加内容。
