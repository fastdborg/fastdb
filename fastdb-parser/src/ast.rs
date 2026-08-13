//! Engine-independent syntax tree for the FastDB language subset.

use miette::SourceSpan;

/// A half-open byte range in the original UTF-8 source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub offset: usize,
    pub len: usize,
}

impl Span {
    pub const fn new(offset: usize, len: usize) -> Self {
        Self { offset, len }
    }

    pub const fn end(self) -> usize {
        self.offset + self.len
    }

    pub const fn is_within(self, input_len: usize) -> bool {
        self.offset <= input_len && self.len <= input_len.saturating_sub(self.offset)
    }

    pub fn to_source_span(self) -> SourceSpan {
        SourceSpan::new(self.offset.into(), self.len)
    }

    pub fn union(self, other: Span) -> Span {
        let start = self.offset.min(other.offset);
        let end = self.end().max(other.end());
        Span::new(start, end.saturating_sub(start))
    }
}

/// A value paired with its source range.
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub const fn new(value: T, span: Span) -> Self {
        Self { value, span }
    }
}

pub type Identifier = Spanned<String>;

#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    pub statements: Vec<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Create(CreateStatement),
    Relate(RelateStatement),
    Select(SelectStatement),
    Update(UpdateStatement),
    Delete(DeleteStatement),
    DefineTable(DefineTableStatement),
    DefineField(DefineFieldStatement),
    DefineAnalyzer(DefineAnalyzerStatement),
    DefineIndex(DefineIndexStatement),
    Explain(ExplainStatement),
    RemoveIndex(IndexMaintenanceStatement),
    RebuildIndex(IndexMaintenanceStatement),
    Begin(TransactionStatement),
    Commit(TransactionStatement),
    Cancel(TransactionStatement),
}

impl Statement {
    pub const fn span(&self) -> Span {
        match self {
            Self::Create(stmt) => stmt.span,
            Self::Relate(stmt) => stmt.span,
            Self::Select(stmt) => stmt.span,
            Self::Update(stmt) => stmt.span,
            Self::Delete(stmt) => stmt.span,
            Self::DefineTable(stmt) => stmt.span,
            Self::DefineField(stmt) => stmt.span,
            Self::DefineAnalyzer(stmt) => stmt.span,
            Self::DefineIndex(stmt) => stmt.span,
            Self::Explain(stmt) => stmt.span,
            Self::RemoveIndex(stmt) | Self::RebuildIndex(stmt) => stmt.span,
            Self::Begin(stmt) | Self::Commit(stmt) | Self::Cancel(stmt) => stmt.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateStatement {
    pub span: Span,
    pub only: Option<Span>,
    pub target: Target,
    pub data: CreateData,
    pub return_clause: Option<ReturnClause>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CreateData {
    Content(Expr),
    Set(Vec<Assignment>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RelateStatement {
    pub span: Span,
    pub only: Option<Span>,
    pub from: Expr,
    pub relation: Identifier,
    pub to: Expr,
    pub data: Option<CreateData>,
    pub return_clause: Option<ReturnClause>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectStatement {
    pub span: Span,
    pub projections: ProjectionList,
    pub only: Option<Span>,
    pub target: Target,
    pub condition: Option<Expr>,
    pub order_by: Vec<OrderBy>,
    pub limit: Option<NonnegativeInteger>,
    pub start: Option<NonnegativeInteger>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProjectionList {
    All(Span),
    Fields(Vec<Projection>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Projection {
    pub span: Span,
    pub expression: Expr,
    pub alias: Option<Identifier>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderBy {
    pub span: Span,
    pub path: FieldPath,
    pub direction: Spanned<OrderDirection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NonnegativeInteger {
    pub value: u64,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateStatement {
    pub span: Span,
    pub target: Target,
    pub assignments: Vec<Assignment>,
    pub condition: Option<Expr>,
    pub return_clause: Option<ReturnClause>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeleteStatement {
    pub span: Span,
    pub target: Target,
    pub condition: Option<Expr>,
    pub return_clause: Option<ReturnClause>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub span: Span,
    pub path: FieldPath,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReturnClause {
    pub span: Span,
    pub kind: Spanned<ReturnKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnKind {
    After,
    None,
    Before,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DefineTableStatement {
    pub span: Span,
    pub name: Identifier,
    pub mode: Spanned<TableMode>,
    pub kind: TableKindSyntax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableMode {
    Schemaless,
    Schemafull,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TableKindSyntax {
    Normal { type_span: Option<Span> },
    Relation(RelationTableType),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RelationTableType {
    pub span: Span,
    pub input: Option<Identifier>,
    pub output: Option<Identifier>,
    pub enforced: Option<Span>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DefineFieldStatement {
    pub span: Span,
    pub path: FieldPath,
    pub table_keyword: Option<Span>,
    pub table: Identifier,
    pub ty: SchemaType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DefineAnalyzerStatement {
    pub span: Span,
    pub name: Identifier,
    pub tokenizer: Spanned<AnalyzerTokenizerSyntax>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyzerTokenizerSyntax {
    Blank,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DefineIndexStatement {
    pub span: Span,
    pub name: Identifier,
    pub table_keyword: Option<Span>,
    pub table: Identifier,
    pub fields: Vec<FieldPath>,
    pub unique: Option<Span>,
    pub kind: IndexKindSyntax,
    pub surface: IndexDefinitionSurface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexDefinitionSurface {
    SurrealDefine,
    FastDbCreate,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IndexKindSyntax {
    Btree,
    Fulltext {
        span: Span,
        analyzer: Identifier,
        highlights: Option<Span>,
    },
    Provider {
        span: Span,
        name: Identifier,
        options: Vec<IndexOption>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexOption {
    pub span: Span,
    pub key: Identifier,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExplainStatement {
    pub span: Span,
    pub select: SelectStatement,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexMaintenanceStatement {
    pub span: Span,
    pub name: Identifier,
    pub table_keyword: Option<Span>,
    pub table: Identifier,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SchemaType {
    pub span: Span,
    pub kind: SchemaTypeKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SchemaTypeKind {
    Bool,
    Int,
    Float,
    Number,
    Decimal,
    String,
    Bytes,
    Datetime,
    Duration,
    Uuid,
    Regex,
    File,
    Table,
    Object,
    Array,
    TypedArray {
        element: Box<SchemaType>,
        length: Option<NonnegativeInteger>,
    },
    FixedFloatArray(NonnegativeInteger),
    Set {
        element: Option<Box<SchemaType>>,
        length: Option<NonnegativeInteger>,
    },
    Range,
    Record,
    Option(Box<SchemaType>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransactionStatement {
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Table(TableTarget),
    Record(RecordId),
}

impl Target {
    pub const fn span(&self) -> Span {
        match self {
            Self::Table(target) => target.span,
            Self::Record(target) => target.span,
        }
    }

    pub fn table(&self) -> &Identifier {
        match self {
            Self::Table(target) => &target.name,
            Self::Record(target) => &target.table,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableTarget {
    pub span: Span,
    pub name: Identifier,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordId {
    pub span: Span,
    pub table: Identifier,
    pub id: RecordIdPart,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordIdPart {
    pub span: Span,
    pub kind: RecordIdPartKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecordIdPartKind {
    Bare(String),
    Quoted(String),
    Integer(i64),
    Uuid(uuid::Uuid),
}

impl RecordIdPart {
    /// Render the component as source accepted by this parser.
    pub fn to_source(&self) -> String {
        match &self.kind {
            RecordIdPartKind::Bare(value) => value.clone(),
            RecordIdPartKind::Quoted(value) => format!("`{}`", value.replace('`', "``")),
            RecordIdPartKind::Integer(value) => value.to_string(),
            RecordIdPartKind::Uuid(value) => format!("u'{}'", value.hyphenated()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldPath {
    pub segments: Vec<Identifier>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub span: Span,
    pub kind: ExprKind,
}

impl Expr {
    pub const fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    None,
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    Duration(String),
    String(String),
    Array(Vec<Expr>),
    Object(Vec<ObjectField>),
    Parameter(String),
    RecordId(RecordId),
    FieldPath(FieldPath),
    Access {
        target: Box<Expr>,
        accessor: Accessor,
    },
    Cast {
        ty: SchemaType,
        value: Box<Expr>,
    },
    Range(RangeExpr),
    FunctionCall {
        name: Vec<Identifier>,
        arguments: Vec<Expr>,
    },
    NamespacedValue {
        name: Vec<Identifier>,
    },
    Knn(KnnExpr),
    Traversal(TraversalExpr),
    Unary {
        operator: Spanned<UnaryOperator>,
        operand: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        operator: Spanned<BinaryOperator>,
        right: Box<Expr>,
    },
    Parenthesized(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Accessor {
    Field(Identifier),
    Index(Box<Expr>),
    Last(Span),
    Slice {
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
        inclusive: bool,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RangeExpr {
    pub start: Option<Box<Expr>>,
    pub end: Option<Box<Expr>>,
    pub inclusive: bool,
    pub operator_span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KnnExpr {
    pub field: Box<Expr>,
    pub k: NonnegativeInteger,
    pub metric: Spanned<KnnMetric>,
    pub query: Box<Expr>,
    pub operator_span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnnMetric {
    Cosine,
    Euclidean,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraversalExpr {
    pub hops: Vec<TraversalHop>,
    pub materialize: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraversalHop {
    pub span: Span,
    pub direction: Spanned<TraversalDirection>,
    pub relation: Identifier,
    pub endpoint_table: Identifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalDirection {
    Forward,
    Reverse,
    Bidirectional,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectField {
    pub span: Span,
    pub key: ObjectKey,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectKey {
    pub span: Span,
    pub kind: ObjectKeyKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ObjectKeyKind {
    Identifier(String),
    String(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperator {
    Not,
    Plus,
    Minus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Power,
    Multiply,
    Divide,
    Modulo,
    Add,
    Subtract,
    Contains,
    ContainsNot,
    ContainsAll,
    ContainsAny,
    ContainsNone,
    Inside,
    NotInside,
    AllInside,
    AnyInside,
    NoneInside,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    ExactEqual,
    AnyEqual,
    AllEqual,
    NotEqual,
    FtsMatch(Option<u32>),
    And,
    Or,
    NullCoalesce,
    TruthyCoalesce,
}
