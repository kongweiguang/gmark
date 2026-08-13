<!-- @author kongweiguang -->

# 代码示例

这份页面参考了其它 Markdown 工具文档中的代码样板，重点观察行内代码、语言标记、语法高亮、长行换行和不同风格代码块的表现。

## 行内代码

命令 `cargo test -p gmark-markdown`、函数 `render_markdown()`、变量 `document_source` 和路径 `gmark-docs/README.md` 都适合用行内代码标记。

## JavaScript

```javascript
function greet(name) {
  return `Hello, ${name}!`;
}

console.log(greet("Gmark"));
```

## TypeScript 与 TSX

```typescript
type User = {
  id: number;
  name: string;
  active: boolean;
};

const user: User = {
  id: 1,
  name: "Alice",
  active: true,
};

console.log(user);
```

```tsx
type CounterProps = { initial?: number };

export function Counter({ initial = 0 }: CounterProps) {
  const [count, setCount] = React.useState(initial);
  return <button onClick={() => setCount(count + 1)}>{count}</button>;
}
```

## Python

```python
def fibonacci(n: int) -> list[int]:
    values = [0, 1]
    while len(values) < n:
        values.append(values[-1] + values[-2])
    return values[:n]


print(fibonacci(8))
```

## Bash 与 Shell

```bash
set -euo pipefail

mkdir -p gmark-docs
printf 'rendering demo\n' > gmark-docs/status.txt
```

```sh
#!/usr/bin/env sh
project_name="${1:-demo}"
printf 'creating %s\n' "$project_name"
```

## JSON、YAML 与 TOML

```json
{
  "name": "gmark-docs",
  "kind": "rendering-showcase",
  "enabled": true
}
```

```yaml
name: markdown-demo
checks:
  - source
  - live
  - preview
```

```toml
[package]
name = "gmark-docs"
edition = "2024"
```

## SQL

```sql
SELECT
  author_id,
  COUNT(*) AS document_count,
  MAX(updated_at) AS latest_update
FROM documents
WHERE status = 'published'
GROUP BY author_id
ORDER BY latest_update DESC;
```

## HTML 与 CSS

```html
<section class="hero">
  <h1>Gmark Docs</h1>
  <p>Write, inspect, and render Markdown locally.</p>
</section>
```

```css
:root {
  --surface: #f8fafc;
  --ink: #0f172a;
  --accent: #2563eb;
}

.hero {
  padding: 2rem;
  color: var(--ink);
  background: var(--surface);
  border: 1px solid #cbd5e1;
}
```

## HTTP 请求

```http
POST /api/documents HTTP/1.1
Host: example.test
Content-Type: application/json

{
  "title": "Rendering demo",
  "published": false
}
```

## Dockerfile

```dockerfile
FROM rust:1.95

WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
RUN cargo fetch --locked

COPY . .
RUN cargo test -p gmark-markdown --locked
```

## Go 与 Rust

```go
package main

import "fmt"

func main() {
    fmt.Println("Hello, Gmark")
}
```

```rust
fn sum(values: &[i32]) -> i32 {
    values.iter().sum()
}

fn main() {
    let numbers = [1, 2, 3, 4];
    println!("sum = {}", sum(&numbers));
}
```

## Diff 与纯文本

```diff
- const mode = "draft";
+ const mode = "published";

- console.log("old content");
+ console.log("new content");
```

```text
POST /health HTTP/1.1
Host: localhost:8080
Accept: application/json
```

## 代码块建议

- 给代码块加上准确的语言标记，便于高亮和快速识别。
- 一段代码只表达一个重点，说明文字放在代码块前后。
- 配置、请求和 Diff 可以分别使用 `json`、`http` 和 `diff` 标记。

下一页：[数学公式](./03-math.md)。
