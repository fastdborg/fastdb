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
    Insert(InsertStatement),
    Upsert(UpdateStatement),
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
    Let(LetStatement),
    ScriptReturn(ScriptExpressionStatement),
    If(IfStatement),
    For(ForStatement),
    Break(ControlFlowStatement),
    Continue(ControlFlowStatement),
    Throw(ScriptExpressionStatement),
    Sleep(ScriptExpressionStatement),
    DefineParam(DefineParamStatement),
    AlterParam(AlterParamStatement),
    RemoveParam(RemoveParamStatement),
    InfoDatabase(InfoDatabaseStatement),
    Begin(TransactionStatement),
    Commit(TransactionStatement),
    Cancel(TransactionStatement),
}

impl Statement {
    pub const fn span(&self) -> Span {
        match self {
            Self::Create(stmt) => stmt.span,
            Self::Insert(stmt) => stmt.span,
            Self::Upsert(stmt) => stmt.span,
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
            Self::Let(stmt) => stmt.span,
            Self::ScriptReturn(stmt) | Self::Throw(stmt) | Self::Sleep(stmt) => stmt.span,
            Self::If(stmt) => stmt.span,
            Self::For(stmt) => stmt.span,
            Self::Break(stmt) | Self::Continue(stmt) => stmt.span,
            Self::DefineParam(stmt) => stmt.span,
            Self::AlterParam(stmt) => stmt.span,
            Self::RemoveParam(stmt) => stmt.span,
            Self::InfoDatabase(stmt) => stmt.span,
            Self::Begin(stmt) | Self::Commit(stmt) | Self::Cancel(stmt) => stmt.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScriptBlock {
    pub span: Span,
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LetStatement {
    pub span: Span,
    pub name: Identifier,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScriptExpressionStatement {
    pub span: Span,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfStatement {
    pub span: Span,
    pub branches: Vec<(Expr, ScriptBlock)>,
    pub otherwise: Option<ScriptBlock>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForStatement {
    pub span: Span,
    pub binding: Identifier,
    pub iterable: Expr,
    pub body: ScriptBlock,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControlFlowStatement {
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DefineParamStatement {
    pub span: Span,
    pub if_not_exists: Option<Span>,
    pub overwrite: Option<Span>,
    pub name: Identifier,
    pub value: Expr,
    pub permissions: SchemaPermissions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlterParamStatement {
    pub span: Span,
    pub name: Identifier,
    pub value: Option<Expr>,
    pub permissions: Option<SchemaPermissions>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaPermissions {
    Full,
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RemoveParamStatement {
    pub span: Span,
    pub if_exists: Option<Span>,
    pub name: Identifier,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InfoDatabaseStatement {
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateStatement {
    pub span: Span,
    pub only: Option<Span>,
    pub target: Target,
    pub data: Option<CreateData>,
    pub return_clause: Option<ReturnClause>,
    pub timeout: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InsertStatement {
    pub span: Span,
    pub relation: Option<Span>,
    pub ignore: Option<Span>,
    pub table: Identifier,
    pub data: InsertData,
    pub on_duplicate: Vec<Assignment>,
    pub return_clause: Option<ReturnClause>,
    pub timeout: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InsertData {
    Expression(Expr),
    Values {
        fields: Vec<Identifier>,
        rows: Vec<Vec<Expr>>,
    },
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
    pub value: Option<Span>,
    pub projections: ProjectionList,
    pub include_all: bool,
    pub only: Option<Span>,
    pub target: SelectTarget,
    pub additional_targets: Vec<SelectTarget>,
    pub condition: Option<Expr>,
    pub split: Vec<FieldPath>,
    pub group: Option<GroupClause>,
    pub omit: Vec<FieldPath>,
    pub order_by: Vec<OrderBy>,
    pub order_random: Option<Span>,
    pub limit: Option<NonnegativeInteger>,
    pub limit_expression: Option<Expr>,
    pub start: Option<NonnegativeInteger>,
    pub start_expression: Option<Expr>,
    pub fetch: Vec<FieldPath>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SelectTarget {
    Target(Target),
    Expression(Expr),
    Subquery(Box<SelectStatement>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum GroupClause {
    All(Span),
    By(Vec<Expr>),
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
    pub collate: Option<Span>,
    pub numeric: Option<Span>,
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
    pub only: Option<Span>,
    pub target: Target,
    pub data: UpdateData,
    pub condition: Option<Expr>,
    pub return_clause: Option<ReturnClause>,
    pub timeout: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UpdateData {
    Content(Expr),
    Merge(Expr),
    Patch(Expr),
    Replace(Expr),
    Set(Vec<Assignment>),
    Unset(Vec<FieldPath>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeleteStatement {
    pub span: Span,
    pub only: Option<Span>,
    pub target: Target,
    pub condition: Option<Expr>,
    pub return_clause: Option<ReturnClause>,
    pub timeout: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub span: Span,
    pub path: FieldPath,
    pub operator: Spanned<AssignmentOperator>,
    pub value: Expr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentOperator {
    Set,
    Add,
    Subtract,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReturnClause {
    pub span: Span,
    pub kind: Spanned<ReturnKind>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReturnKind {
    After,
    None,
    Before,
    Diff,
    Value(Expr),
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
    pub analyze: Option<Span>,
    pub full: Option<Span>,
    pub format_json: Option<Span>,
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
    RecordRange(RecordRangeTarget),
    Expression(Expr),
    Batch { span: Span, target: Box<Target> },
}

impl Target {
    pub const fn span(&self) -> Span {
        match self {
            Self::Table(target) => target.span,
            Self::Record(target) => target.span,
            Self::RecordRange(target) => target.span,
            Self::Expression(target) => target.span,
            Self::Batch { span, .. } => *span,
        }
    }

    pub fn table(&self) -> Option<&Identifier> {
        match self {
            Self::Table(target) => Some(&target.name),
            Self::Record(target) => Some(&target.table),
            Self::RecordRange(target) => Some(&target.table),
            Self::Expression(_) => None,
            Self::Batch { target, .. } => target.table(),
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
pub struct RecordRangeTarget {
    pub span: Span,
    pub table: Identifier,
    pub start: Option<RecordIdPart>,
    pub end: Option<RecordIdPart>,
    pub inclusive: bool,
    pub operator_span: Span,
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
    /// A collision-safe array or object record-ID component.
    Complex(Box<Expr>),
}

impl RecordIdPart {
    /// Render the component as source accepted by this parser.
    pub fn to_source(&self) -> String {
        match &self.kind {
            RecordIdPartKind::Bare(value) => value.clone(),
            RecordIdPartKind::Quoted(value) => format!("`{}`", value.replace('`', "``")),
            RecordIdPartKind::Integer(value) => value.to_string(),
            RecordIdPartKind::Uuid(value) => format!("u'{}'", value.hyphenated()),
            RecordIdPartKind::Complex(value) => render_record_id_expression(value),
        }
    }
}

fn render_record_id_expression(expression: &Expr) -> String {
    match &expression.kind {
        ExprKind::None => "NONE".into(),
        ExprKind::Null => "NULL".into(),
        ExprKind::Bool(value) => value.to_string(),
        ExprKind::Integer(value) => value.to_string(),
        ExprKind::Float(value) => value.to_string(),
        ExprKind::Duration(value) => value.clone(),
        ExprKind::String(value) => {
            format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
        }
        ExprKind::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(render_record_id_expression)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ExprKind::Object(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|field| {
                    let key = match &field.key.kind {
                        ObjectKeyKind::Identifier(value) => value.clone(),
                        ObjectKeyKind::String(value) => {
                            format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
                        }
                    };
                    format!("{key}: {}", render_record_id_expression(&field.value))
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ExprKind::Unary {
            operator, operand, ..
        } => {
            let operator = match operator.value {
                UnaryOperator::Plus => "+",
                UnaryOperator::Minus => "-",
                UnaryOperator::Not => "!",
            };
            format!("{operator}{}", render_record_id_expression(operand))
        }
        ExprKind::Parenthesized(value) => format!("({})", render_record_id_expression(value)),
        _ => "<invalid-record-id>".into(),
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
    Destructure {
        target: Box<Expr>,
        fields: Vec<Identifier>,
    },
    DestructureList(Vec<Expr>),
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
    Closure(ClosureExpr),
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
pub struct ClosureExpr {
    pub parameters: Vec<Identifier>,
    pub body: Box<Expr>,
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
