// @author kongweiguang

use super::*;

/// 一份公式文档，既可承载可编辑的结构，也可承载完全不透明的源码。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MathDocument {
    /// 可局部操作的结构化公式。
    Structured(MathAst),
    /// 调用方明确要求不分析的公式源码。
    Opaque(OpaqueMath),
}

/// Whether a formula can be edited structurally without losing unknown LaTeX.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MathSupportLevel {
    /// All nodes belong to the supported structure subset.
    Structured,
    /// At least one node must remain source-editable/opaque.
    Opaque,
}

impl MathDocument {
    /// 以宽容、无损的方式解析 LaTeX。
    #[must_use]
    pub fn parse(latex: impl Into<String>) -> Self {
        Self::Structured(MathAst::parse(latex))
    }

    /// 建立一个不进行语法解释的文档。
    #[must_use]
    pub fn opaque(latex: impl Into<String>) -> Self {
        Self::Opaque(OpaqueMath::new(latex))
    }

    /// 返回原样可写回的 LaTeX。
    #[must_use]
    pub fn to_latex(&self) -> String {
        match self {
            Self::Structured(ast) => ast.to_latex(),
            Self::Opaque(source) => source.to_latex(),
        }
    }

    /// 返回结构化 AST；不透明文档没有 AST。
    #[must_use]
    pub const fn ast(&self) -> Option<&MathAst> {
        match self {
            Self::Structured(ast) => Some(ast),
            Self::Opaque(_) => None,
        }
    }

    /// 返回可变结构化 AST；不透明文档没有 AST。
    pub fn ast_mut(&mut self) -> Option<&mut MathAst> {
        match self {
            Self::Structured(ast) => Some(ast),
            Self::Opaque(_) => None,
        }
    }

    #[must_use]
    pub fn is_structured(&self) -> bool {
        self.ast().is_some()
    }

    /// Reports whether the formula can enter the two-dimensional editor.
    #[must_use]
    pub fn support_level(&self) -> MathSupportLevel {
        match self {
            Self::Structured(ast) => ast.support_level(),
            Self::Opaque(_) => MathSupportLevel::Opaque,
        }
    }

    /// 用一个 UTF-8 字节范围替换源码。结构化文档会重新建立保真 AST；不透明
    /// 文档仍保持不透明，避免一次普通编辑意外改变调用方的呈现策略。
    pub fn replace_latex_range(
        &mut self,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<(), MathEditError> {
        let source = self.to_latex();
        validate_range(&source, &range)?;
        let mut next = source;
        next.replace_range(range, replacement);
        match self {
            Self::Structured(ast) => *ast = MathAst::parse(next),
            Self::Opaque(opaque) => *opaque = OpaqueMath::new(next),
        }
        Ok(())
    }

    /// Execute one source-oriented command from the beginning of the
    /// document.  Hosts that need a persistent caret should use
    /// [`MathEditor`]; this convenience keeps small integrations allocation
    /// free at the API boundary and still returns an undo-friendly result.
    pub fn apply_command(
        &mut self,
        command: MathEditCommand,
    ) -> Result<MathEditResult, MathEditError> {
        let mut editor = MathEditor::new(self.clone());
        let result = editor.execute(command)?;
        *self = editor.into_document();
        Ok(result)
    }

    #[must_use]
    pub fn editor(&self) -> MathEditor {
        MathEditor::new(self.clone())
    }

    /// 返回文档源码长度（UTF-8 字节）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.to_latex().len()
    }

    /// 判断文档是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.to_latex().is_empty()
    }
}

/// 完全不透明的公式源码。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpaqueMath {
    source: String,
}

impl OpaqueMath {
    #[must_use]
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
        }
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub fn to_latex(&self) -> String {
        self.source.clone()
    }
}

/// 结构化公式的根节点。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MathAst {
    root: MathNode,
}

impl MathAst {
    /// 从 LaTeX 解析一个保真 AST。
    #[must_use]
    pub fn parse(latex: impl Into<String>) -> Self {
        let latex = latex.into();
        Self {
            root: parser::parse(&latex),
        }
    }

    /// 从调用方已有的根节点建立 AST。
    #[must_use]
    pub fn new(root: MathNode) -> Self {
        Self { root }
    }

    #[must_use]
    pub fn root(&self) -> &MathNode {
        &self.root
    }

    pub fn root_mut(&mut self) -> &mut MathNode {
        &mut self.root
    }

    #[must_use]
    pub fn paths(&self) -> Vec<MathPath> {
        let mut paths = Vec::new();
        collect_paths(&self.root, &MathPath::root(), &mut paths);
        paths
    }

    #[must_use]
    pub fn to_latex(&self) -> String {
        self.root.to_latex()
    }

    #[must_use]
    pub fn support_level(&self) -> MathSupportLevel {
        self.root.support_level()
    }

    /// 返回由从根开始的子节点索引组成的路径对应节点。
    #[must_use]
    pub fn node(&self, path: &MathPath) -> Option<&MathNode> {
        node_at(&self.root, path.indices())
    }

    /// Returns the byte range occupied by a node in the canonical source
    /// emitted by this AST.  Parsing is lossless for supported constructs, so
    /// this range is also suitable for structural selections before an edit.
    #[must_use]
    pub fn source_range(&self, path: &MathPath) -> Option<Range<usize>> {
        source_range_at(&self.root, path.indices(), 0)
    }

    /// Enumerates editable cells of a matrix-like environment.  Unknown
    /// environments are intentionally included: callers can navigate and
    /// edit cells while the original begin/end spelling remains untouched.
    #[must_use]
    pub fn environment_slots(&self, path: &MathPath) -> Vec<MathSlot> {
        let Some(MathNode::Environment { .. }) = self.node(path) else {
            return Vec::new();
        };
        let source = self.to_latex();
        let Some(range) = self.source_range(path) else {
            return Vec::new();
        };
        environment_grid(&source[range]).map_or_else(Vec::new, |grid| {
            grid.cells
                .iter()
                .map(|cell| MathSlot::environment_cell(path.clone(), cell.row, cell.column))
                .collect()
        })
    }

    pub fn environment_cell(
        &self,
        path: &MathPath,
        row: usize,
        column: usize,
    ) -> Result<MathSlot, MathEditError> {
        let slot = MathSlot::environment_cell(path.clone(), row, column);
        let source = self.to_latex();
        environment_cell_range(&source, &slot)?;
        Ok(slot)
    }

    /// Select one whole AST node, including its delimiters.
    pub fn select(&self, path: &MathPath) -> Result<MathSelection, MathEditError> {
        let range = self
            .source_range(path)
            .ok_or_else(|| MathEditError::UnknownPath(path.clone()))?;
        Ok(MathSelection::structural(path.clone(), range))
    }

    pub fn select_node(&self, path: &MathPath) -> Result<MathSelection, MathEditError> {
        self.select(path)
    }

    /// 将某个节点完整替换为新节点。
    pub fn replace(&mut self, path: &MathPath, replacement: MathNode) -> Result<(), MathEditError> {
        replace_node(&mut self.root, path.indices(), replacement)
    }

    /// 从其父序列中移除一个节点。根节点不能删除。
    pub fn remove(&mut self, path: &MathPath) -> Result<MathNode, MathEditError> {
        let Some((&index, parent_path)) = path.indices().split_last() else {
            return Err(MathEditError::RootOperation);
        };
        let parent = node_at_mut(&mut self.root, parent_path)
            .ok_or_else(|| MathEditError::UnknownPath(path.clone()))?;
        let children = parent
            .sequence_mut()
            .ok_or_else(|| MathEditError::ParentIsNotSequence(path.clone()))?;
        children
            .get(index)
            .ok_or_else(|| MathEditError::UnknownPath(path.clone()))?;
        Ok(children.remove(index))
    }

    /// 在路径节点之前插入一个兄弟节点。
    pub fn insert_before(&mut self, path: &MathPath, node: MathNode) -> Result<(), MathEditError> {
        self.insert_sibling(path, node, false)
    }

    /// 在路径节点之后插入一个兄弟节点。
    pub fn insert_after(&mut self, path: &MathPath, node: MathNode) -> Result<(), MathEditError> {
        self.insert_sibling(path, node, true)
    }

    fn insert_sibling(
        &mut self,
        path: &MathPath,
        node: MathNode,
        after: bool,
    ) -> Result<(), MathEditError> {
        let Some((&index, parent_path)) = path.indices().split_last() else {
            return Err(MathEditError::RootOperation);
        };
        let parent = node_at_mut(&mut self.root, parent_path)
            .ok_or_else(|| MathEditError::UnknownPath(path.clone()))?;
        let children = parent
            .sequence_mut()
            .ok_or_else(|| MathEditError::ParentIsNotSequence(path.clone()))?;
        if index >= children.len() {
            return Err(MathEditError::UnknownPath(path.clone()));
        }
        let position = if after { index + 1 } else { index };
        children.insert(position, node);
        Ok(())
    }
}

/// The eight delimiter families understood by the math editor.
///
/// The associated constants below intentionally include common spelling
/// aliases (`Paren`, `Ceiling`, `Absolute`, ...).  They are aliases, not extra
/// semantic variants, so exhaustive matches remain stable as the public API
/// grows.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MathDelimiterPair {
    Parentheses,
    Brackets,
    Braces,
    AbsoluteValue,
    Norm,
    Angle,
    Floor,
    Ceil,
}

// Reason: compatibility spellings are associated constants; remove after the pre-1.0 host API window closes.
#[allow(non_upper_case_globals)]
impl MathDelimiterPair {
    pub const Parenthesis: Self = Self::Parentheses;
    pub const Paren: Self = Self::Parentheses;
    pub const Round: Self = Self::Parentheses;
    pub const SquareBrackets: Self = Self::Brackets;
    pub const Bracket: Self = Self::Brackets;
    pub const CurlyBraces: Self = Self::Braces;
    pub const Brace: Self = Self::Braces;
    pub const Absolute: Self = Self::AbsoluteValue;
    pub const Bars: Self = Self::AbsoluteValue;
    pub const VerticalBars: Self = Self::AbsoluteValue;
    pub const AbsoluteBars: Self = Self::AbsoluteValue;
    pub const DoubleBars: Self = Self::Norm;
    pub const DoubleVerticalBars: Self = Self::Norm;
    pub const AngleBrackets: Self = Self::Angle;
    pub const Ceiling: Self = Self::Ceil;

    /// Return the LaTeX tokens used after `\left` and `\right`.
    #[must_use]
    pub const fn tokens(self) -> (&'static str, &'static str) {
        match self {
            Self::Parentheses => ("(", ")"),
            Self::Brackets => ("[", "]"),
            Self::Braces => (r"\{", r"\}"),
            Self::AbsoluteValue => ("|", "|"),
            Self::Norm => (r"\|", r"\|"),
            Self::Angle => (r"\langle", r"\rangle"),
            Self::Floor => (r"\lfloor", r"\rfloor"),
            Self::Ceil => (r"\lceil", r"\rceil"),
        }
    }

    #[must_use]
    pub const fn open(self) -> &'static str {
        self.tokens().0
    }

    #[must_use]
    pub const fn close(self) -> &'static str {
        self.tokens().1
    }

    /// Serialize a body with the command boundary required by alphabetic
    /// delimiter commands such as `\\langle` and `\\lfloor`.
    #[must_use]
    pub fn wrap_body(self, body: &str) -> String {
        let separator = self
            .open()
            .chars()
            .last()
            .is_some_and(|character| character.is_ascii_alphabetic())
            && !body.chars().next().is_some_and(char::is_whitespace);
        if separator {
            format!(r"\left{} {body}\right{}", self.open(), self.close())
        } else {
            format!(r"\left{}{body}\right{}", self.open(), self.close())
        }
    }

    /// Match the token pair found in a `\left...\right...` expression.
    #[must_use]
    pub fn from_tokens(open: &str, close: &str) -> Option<Self> {
        Self::all()
            .into_iter()
            .find(|pair| pair.open() == open && pair.close() == close)
    }

    #[must_use]
    pub const fn all() -> [Self; 8] {
        [
            Self::Parentheses,
            Self::Brackets,
            Self::Braces,
            Self::AbsoluteValue,
            Self::Norm,
            Self::Angle,
            Self::Floor,
            Self::Ceil,
        ]
    }
}

impl TryFrom<(&str, &str)> for MathDelimiterPair {
    type Error = ();

    fn try_from(tokens: (&str, &str)) -> Result<Self, Self::Error> {
        Self::from_tokens(tokens.0, tokens.1).ok_or(())
    }
}

impl TryFrom<char> for MathDelimiterPair {
    type Error = ();

    fn try_from(character: char) -> Result<Self, Self::Error> {
        match character {
            '(' => Ok(Self::Parentheses),
            '[' => Ok(Self::Brackets),
            '{' => Ok(Self::Braces),
            '|' => Ok(Self::AbsoluteValue),
            _ => Err(()),
        }
    }
}

/// AST 节点；每个变体都可无损地重新写成 LaTeX。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MathNode {
    Sequence(Vec<MathNode>),
    Text(String),
    Command {
        name: String,
    },
    Group(Box<MathNode>),
    Fraction {
        numerator: Box<MathNode>,
        denominator: Box<MathNode>,
    },
    SquareRoot {
        index: Option<Box<MathNode>>,
        radicand: Box<MathNode>,
    },
    /// A pair of auto-sized delimiters around an editable body.
    ///
    /// The pair is kept as a semantic value rather than two arbitrary command
    /// strings so palette commands and renderers can share the same eight
    /// supported delimiter families.  The body remains a normal AST node,
    /// which means nested structures retain stable paths and can be edited.
    Delimited {
        pair: MathDelimiterPair,
        body: Box<MathNode>,
    },
    Superscript(Box<MathNode>),
    Subscript(Box<MathNode>),
    /// A `\text{...}` node.  The inner node is retained as a sequence so
    /// nested braces and commands remain editable without flattening them.
    TextMode(Box<MathNode>),
    /// A semantic symbol such as `\alpha` or `\leq`.
    Symbol {
        name: String,
    },
    /// A one-argument accent command (`\hat`, `\vec`, ...).
    Accent {
        name: String,
        value: Box<MathNode>,
    },
    /// A large operator.  Scripts remain ordinary sibling nodes, which keeps
    /// the source order stable while still exposing the operator to renderers.
    BigOperator {
        name: String,
    },
    /// A portable matrix/cases/aligned environment retained losslessly.
    Environment {
        name: String,
        raw: String,
    },
    /// 无法可靠结构化的完整源码片段，例如未闭合分组。
    Opaque(String),
}

impl MathNode {
    #[must_use]
    pub fn sequence(children: Vec<Self>) -> Self {
        Self::Sequence(children)
    }

    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    #[must_use]
    pub fn opaque(source: impl Into<String>) -> Self {
        Self::Opaque(source.into())
    }

    #[must_use]
    pub fn text_mode(content: impl Into<String>) -> Self {
        Self::TextMode(Box::new(Self::text(content)))
    }

    #[must_use]
    pub fn symbol(name: impl Into<String>) -> Self {
        Self::Symbol { name: name.into() }
    }

    #[must_use]
    pub fn accent(name: impl Into<String>, value: Self) -> Self {
        Self::Accent {
            name: name.into(),
            value: Box::new(value),
        }
    }

    #[must_use]
    pub fn big_operator(name: impl Into<String>) -> Self {
        Self::BigOperator { name: name.into() }
    }

    #[must_use]
    pub fn delimited(pair: MathDelimiterPair, body: Self) -> Self {
        Self::Delimited {
            pair,
            body: Box::new(body),
        }
    }

    #[must_use]
    pub const fn delimiter_pair(&self) -> Option<MathDelimiterPair> {
        match self {
            Self::Delimited { pair, .. } => Some(*pair),
            _ => None,
        }
    }

    /// Returns children that can be addressed by a structural path.  A
    /// sequence exposes all members; compound nodes expose their slots in
    /// source order.  Leaf nodes return an empty slice.
    #[must_use]
    pub fn children(&self) -> Vec<&Self> {
        match self {
            Self::Sequence(children) => children.iter().collect(),
            Self::Group(content)
            | Self::Superscript(content)
            | Self::Subscript(content)
            | Self::TextMode(content)
            | Self::Delimited { body: content, .. }
            | Self::Accent { value: content, .. } => vec![content],
            Self::Fraction {
                numerator,
                denominator,
            } => vec![numerator, denominator],
            Self::SquareRoot { index, radicand } => {
                let mut children = Vec::with_capacity(index.is_some() as usize + 1);
                if let Some(index) = index {
                    children.push(index.as_ref());
                }
                children.push(radicand);
                children
            }
            Self::Text(_)
            | Self::Command { .. }
            | Self::Symbol { .. }
            | Self::BigOperator { .. }
            | Self::Environment { .. }
            | Self::Opaque(_) => Vec::new(),
        }
    }

    #[must_use]
    pub fn is_leaf(&self) -> bool {
        self.children().is_empty()
    }

    /// 返回精确的 LaTeX 源码。
    #[must_use]
    pub fn to_latex(&self) -> String {
        match self {
            Self::Sequence(children) => children.iter().map(Self::to_latex).collect(),
            Self::Text(text) | Self::Opaque(text) => text.clone(),
            Self::Command { name } => format!("\\{name}"),
            Self::Group(content) => format!("{{{}}}", content.to_latex()),
            Self::Fraction {
                numerator,
                denominator,
            } => format!(
                "\\frac{{{}}}{{{}}}",
                numerator.to_latex(),
                denominator.to_latex()
            ),
            Self::SquareRoot { index, radicand } => {
                let index = index
                    .as_ref()
                    .map(|value| format!("[{}]", value.to_latex()))
                    .unwrap_or_default();
                format!("\\sqrt{index}{{{}}}", radicand.to_latex())
            }
            Self::Delimited { pair, body } => pair.wrap_body(&body.to_latex()),
            Self::Superscript(value) => format!("^{}", script_latex(value)),
            Self::Subscript(value) => format!("_{}", script_latex(value)),
            Self::TextMode(content) => format!("\\text{{{}}}", content.to_latex()),
            Self::Symbol { name } | Self::BigOperator { name } => format!("\\{name}"),
            Self::Accent { name, value } => {
                format!("\\{name}{{{}}}", value.to_latex())
            }
            Self::Environment { raw, .. } => raw.clone(),
        }
    }

    fn support_level(&self) -> MathSupportLevel {
        match self {
            Self::Sequence(children) => children
                .iter()
                .find(|child| child.support_level() == MathSupportLevel::Opaque)
                .map_or(MathSupportLevel::Structured, |_| MathSupportLevel::Opaque),
            Self::Command { name } if !parser::supported_command(name) => MathSupportLevel::Opaque,
            Self::Command { .. } => MathSupportLevel::Structured,
            Self::Group(content) | Self::Superscript(content) | Self::Subscript(content) => {
                content.support_level()
            }
            Self::TextMode(content) | Self::Accent { value: content, .. } => {
                content.support_level()
            }
            Self::Fraction {
                numerator,
                denominator,
            } => {
                if numerator.support_level() == MathSupportLevel::Opaque
                    || denominator.support_level() == MathSupportLevel::Opaque
                {
                    MathSupportLevel::Opaque
                } else {
                    MathSupportLevel::Structured
                }
            }
            Self::SquareRoot { index, radicand } => {
                if index
                    .as_deref()
                    .is_some_and(|node| node.support_level() == MathSupportLevel::Opaque)
                    || radicand.support_level() == MathSupportLevel::Opaque
                {
                    MathSupportLevel::Opaque
                } else {
                    MathSupportLevel::Structured
                }
            }
            Self::Delimited { body, .. } => body.support_level(),
            Self::Text(_) | Self::Symbol { .. } | Self::BigOperator { .. } => {
                MathSupportLevel::Structured
            }
            Self::Environment { name, raw } => {
                let Some(grid) = environment_grid(raw) else {
                    return MathSupportLevel::Opaque;
                };
                if !parser::supported_environment(name)
                    || grid.cells.iter().any(|cell| {
                        parser::parse(&raw[cell.start..cell.end]).support_level()
                            == MathSupportLevel::Opaque
                    })
                {
                    MathSupportLevel::Opaque
                } else {
                    MathSupportLevel::Structured
                }
            }
            Self::Opaque(_) => MathSupportLevel::Opaque,
        }
    }

    fn sequence_mut(&mut self) -> Option<&mut Vec<MathNode>> {
        match self {
            Self::Sequence(children) => Some(children),
            _ => None,
        }
    }
}

fn script_latex(value: &MathNode) -> String {
    match value {
        MathNode::Group(content) => format!("{{{}}}", content.to_latex()),
        _ => value.to_latex(),
    }
}

/// 一个稳定的 AST 节点路径。空路径表示根节点。
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct MathPath(pub(super) Vec<usize>);

impl MathPath {
    #[must_use]
    pub const fn root() -> Self {
        Self(Vec::new())
    }

    #[must_use]
    pub fn child(&self, index: usize) -> Self {
        let mut indices = self.0.clone();
        indices.push(index);
        Self(indices)
    }

    #[must_use]
    pub fn indices(&self) -> &[usize] {
        &self.0
    }

    #[must_use]
    pub fn from_indices(indices: impl IntoIterator<Item = usize>) -> Self {
        Self(indices.into_iter().collect())
    }

    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        (!self.0.is_empty()).then(|| Self(self.0[..self.0.len() - 1].to_vec()))
    }

    #[must_use]
    pub fn last(&self) -> Option<usize> {
        self.0.last().copied()
    }

    #[must_use]
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Vec<usize>> for MathPath {
    fn from(indices: Vec<usize>) -> Self {
        Self(indices)
    }
}

impl FromIterator<usize> for MathPath {
    fn from_iter<T: IntoIterator<Item = usize>>(iter: T) -> Self {
        Self::from_indices(iter)
    }
}
