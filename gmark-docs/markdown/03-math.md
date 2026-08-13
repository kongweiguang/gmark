<!-- @author kongweiguang -->

# 数学公式

本页集中展示 Gmark 的行内数学和块数学。公式正文使用 LaTeX，源码仍然是普通 Markdown，适合在 Source 和 Preview 之间来回检查。

## 行内公式与块公式

短公式可以直接放在段落里：质能方程 $E = mc^2$、圆面积 $S = \pi r^2$、线性模型 $y = \beta_0 + \beta_1x + \varepsilon$。

块公式适合独立展示核心结论：

$$
\int_{-\infty}^{\infty} e^{-x^2}\,dx = \sqrt{\pi}
$$

$$
\sum_{i=1}^{n} i = \frac{n(n+1)}{2}
$$

## 链式法则推导

设复合函数：

$$
y = \sin(x^2 + 1)
$$

引入中间变量：

$$
u = x^2 + 1, \qquad y = \sin u
$$

根据链式法则：

$$
\frac{dy}{dx} = \frac{dy}{du}\cdot\frac{du}{dx}
$$

因此：

$$
\frac{dy}{dx} = \cos(x^2 + 1)\cdot 2x
$$

> [!TIP]
> 推导超过三步时，把每一步拆成独立块公式，阅读和定位都会更稳定。

## 分段函数

绝对值和 ReLU 可以写成分段函数：

$$
|x| =
\begin{cases}
x, & x \ge 0 \\
-x, & x < 0
\end{cases}
$$

$$
\operatorname{ReLU}(x) =
\begin{cases}
x, & x > 0 \\
0, & x \le 0
\end{cases}
$$

## 矩阵与向量

$$
\mathbf{x} =
\begin{bmatrix}
x_1 \\
x_2 \\
\vdots \\
x_n
\end{bmatrix},
\qquad
\mathbf{A} =
\begin{bmatrix}
a_{11} & a_{12} \\
a_{21} & a_{22}
\end{bmatrix}
$$

矩阵乘法：

$$
\mathbf{A}\mathbf{x} =
\begin{bmatrix}
a_{11} & a_{12} \\
a_{21} & a_{22}
\end{bmatrix}
\begin{bmatrix}
x_1 \\
x_2
\end{bmatrix}
=
\begin{bmatrix}
a_{11}x_1 + a_{12}x_2 \\
a_{21}x_1 + a_{22}x_2
\end{bmatrix}
$$

## 求和、积分与极限

$$
\lim_{x \to 0}\frac{\sin x}{x} = 1
$$

$$
\int_{0}^{1} x^2\,dx = \frac{1}{3}
$$

$$
\sum_{k=0}^{\infty} ar^k = \frac{a}{1-r}, \qquad |r| < 1
$$

$$
\mathbb{E}[X] = \sum_x xP(X=x)
\qquad\text{或}\qquad
\mathbb{E}[X] = \int_{-\infty}^{\infty} xf(x)\,dx
$$

## 表格中的短公式

| 类型 | 示例公式 | 备注 |
| --- | --- | --- |
| 导数 | $\frac{d}{dx}x^n = nx^{n-1}$ | 适合短公式 |
| 积分 | $\int e^x\,dx = e^x + C$ | 避免太长 |
| 概率 | $P(A \mid B) = \frac{P(A \cap B)}{P(B)}$ | 适合速查 |
| 线代 | $\mathbf{A}\mathbf{x} = \mathbf{b}$ | 适合定义 |

> [!WARNING]
> 表格更适合短公式。矩阵展开、分段函数和多步推导应单独使用块公式。

## 化学与物理记号

行内上下标也适合简单化学式和物理记号：$H_2O$、$CO_2$、$v_0$、$x_{n+1}$、$\alpha$ 和 $\Delta t$。

下一页：[Mermaid 图表](./04-mermaid.md)。
