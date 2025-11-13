// ---------- AST ----------

/// A node in the Rollatorium abstract syntax tree.
///
/// The grammar for the language supports a rich set of operations for dice
/// expressions (selectors, modifiers, annotations, etc.).  The `Node` enum is
/// intentionally expressive enough to represent those constructs, even though
/// the current parser only emits a small subset today.  The extra variants and
/// supporting types make it possible to extend the parser without having to
/// redesign the tree structure later on.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// A numeric literal.
    Literal(f64),
    /// A unary operation such as negation.
    Unary {
        operator: UnaryOperator,
        operand: Box<Node>,
    },
    /// A binary arithmetic operation (addition, multiplication, etc.).
    Binary {
        operator: BinaryOperator,
        left: Box<Node>,
        right: Box<Node>,
    },
    /// A dice roll expression, e.g. `4d6` or `d%`.
    Dice {
        num: Option<Box<Node>>,
        size: DiceSize,
    },
    /// A set literal (with optional set-style operations).
    Set {
        elements: Vec<Node>,
        operations: Vec<SetOperation>,
    },
    /// A dice expression with additional keep/drop/reroll/etc. operations.
    DiceWithOps {
        dice: Box<Node>,
        operations: Vec<SetOperation>,
    },
    /// An annotated expression, e.g. `4d6 [strength]`.
    Annotated {
        expr: Box<Node>,
        annotations: Vec<Annotation>,
    },
}

/// The size of a die (e.g. 6 for d6 or percent for d%).
#[derive(Debug, Clone, PartialEq)]
pub enum DiceSize {
    Value(Box<Node>),
    Percent,
}

/// Unary operators supported by the language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperator {
    Plus,
    Minus,
}

/// Binary operators supported by the language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    IntDivide,
    Modulo,
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
}

/// A selector targets a subset of a dice pool (e.g. highest, lowest).
#[derive(Debug, Clone, PartialEq)]
pub struct Selector {
    pub kind: SelectorKind,
    pub target: Box<Node>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorKind {
    Literal,
    Highest,
    Lowest,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    EqualTo,
    NotEqual,
}

/// The different set operations that can be applied to a dice pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetOperator {
    Keep,
    Drop,
    Reroll,
    RerollOnce,
    RerollAdd,
    Explode,
    ExplodeCompound,
    ExplodePenetrate,
    Penetrate,
    Minimum,
    Maximum,
    CountSuccess,
    CountFailure,
}

/// A modifier applied to a dice set, potentially using a selector.
#[derive(Debug, Clone, PartialEq)]
pub struct SetOperation {
    pub operator: SetOperator,
    pub selectors: Vec<Selector>,
}

/// Represents a textual annotation applied to a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    pub text: String,
}
