//! Closed metadata registry for FastDB-owned built-in functions.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuiltinClass {
    Pure,
    Context,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuiltinSyntax {
    Function,
    Constant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Builtin {
    ArrayAdd,
    ArrayAppend,
    ArrayAt,
    ArrayComplement,
    ArrayConcat,
    ArrayDifference,
    ArrayDistinct,
    ArrayFirst,
    ArrayIncludes,
    ArrayIndexOf,
    ArrayIntersect,
    ArrayIsEmpty,
    ArrayJoin,
    ArrayLast,
    ArrayLen,
    ArrayMax,
    ArrayMin,
    ArrayPop,
    ArrayPrepend,
    ArrayRange,
    ArrayRemove,
    ArrayRepeat,
    ArrayReverse,
    ArraySlice,
    ArraySort,
    ArraySortAsc,
    ArraySortDesc,
    ArrayUnion,
    ArrayBoolean(ArrayBoolean),
    ArrayClump,
    ArrayCombine,
    ArrayFill,
    ArrayFlatten,
    ArrayGroup,
    ArrayInsert,
    ArrayLogical(ArrayLogical),
    ArrayMatches,
    ArraySequence,
    ArrayShuffle,
    ArraySortLexical,
    ArraySortNatural(bool),
    ArraySwap,
    ArrayTranspose,
    ArrayWindows,
    CollectionClosure(CollectionKind, ClosureOperation),
    BytesLen,
    ObjectEntries,
    ObjectExtend,
    ObjectFromEntries,
    ObjectIsEmpty,
    ObjectKeys,
    ObjectLen,
    ObjectRemove,
    ObjectValues,
    SetAdd,
    SetAt,
    SetComplement,
    SetContains,
    SetDifference,
    SetFirst,
    SetIntersect,
    SetIsEmpty,
    SetJoin,
    SetLast,
    SetLen,
    SetMax,
    SetMin,
    SetRemove,
    SetSlice,
    SetUnion,
    SetFlatten,
    MathConstant(MathConstant),
    MathUnary(MathUnary),
    MathClamp,
    MathLerp,
    MathLog,
    MathMax,
    MathMean,
    MathMin,
    MathPow,
    MathProduct,
    MathSpread,
    MathSum,
    MathBottom,
    MathFixed,
    MathInterquartile,
    MathLerpAngle,
    MathMedian,
    MathMidhinge,
    MathMode,
    MathNearestRank,
    MathPercentile,
    MathStddev,
    MathTop,
    MathTrimean,
    MathVariance,
    TypeCast(TypeCast),
    TypeIs(TypeKind),
    TypeOf,
    TypeField,
    TypeFields,
    RecordId,
    RecordTable,
    DurationMax,
    DurationExtract(DurationUnit),
    DurationFrom(DurationUnit),
    TimeEpoch,
    TimeNow,
    TimeTimezone,
    TimePart(TimePart),
    TimeFrom(DurationUnit),
    TimeIsLeapYear,
    TimeMin,
    TimeMax,
    TimeFormat,
    TimeSet(TimePart),
    TimeTruncate(TimeTruncate),
    TimeFromUuid,
    TimeFromUlid,
    EncodingBase64Encode,
    EncodingBase64Decode,
    EncodingJsonEncode,
    EncodingJsonDecode,
    EncodingCborEncode,
    EncodingCborDecode,
    CryptoDigest(CryptoDigest),
    CryptoJoaat,
    CryptoPassword(PasswordAlgorithm, PasswordOperation),
    Random(RandomBuiltin),
    Count,
    Not,
    ValueExpect,
    String(StringBuiltin),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CollectionKind {
    Array,
    Set,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClosureOperation {
    All,
    Any,
    Filter,
    FilterIndex,
    Find,
    FindIndex,
    Fold,
    Map,
    Reduce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArrayBoolean {
    And,
    Not,
    Or,
    Xor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArrayLogical {
    And,
    Or,
    Xor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PasswordAlgorithm {
    Argon2,
    Bcrypt,
    Pbkdf2,
    Scrypt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PasswordOperation {
    Compare,
    Generate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RandomBuiltin {
    Bool,
    Duration,
    Enum,
    Float,
    Id,
    Int,
    String,
    Time,
    Ulid,
    UuidV4,
    UuidV7,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StringBuiltin {
    Capitalize,
    Concat,
    Contains,
    Distance(StringDistance),
    EndsWith,
    HtmlEncode,
    HtmlSanitize,
    Join,
    Len,
    Lowercase,
    Matches,
    ParseEmailHost,
    ParseEmailUser,
    ParseUrl(StringUrlPart),
    Repeat,
    Replace,
    Reverse,
    Semver(StringSemver),
    Similarity(StringSimilarity),
    Slice,
    Slug,
    Split,
    StartsWith,
    Trim,
    Uppercase,
    Validate(StringValidator),
    Words,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StringDistance {
    DamerauLevenshtein,
    Hamming,
    Levenshtein,
    NormalizedDamerauLevenshtein,
    NormalizedLevenshtein,
    Osa,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StringUrlPart {
    Domain,
    Fragment,
    Host,
    Path,
    Port,
    Query,
    Scheme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StringSemver {
    Compare,
    IncMajor,
    IncMinor,
    IncPatch,
    Major,
    Minor,
    Patch,
    SetMajor,
    SetMinor,
    SetPatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StringSimilarity {
    Fuzzy,
    Jaro,
    JaroWinkler,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StringValidator {
    Alpha,
    Alphanumeric,
    Ascii,
    Datetime,
    Domain,
    Email,
    Hexadecimal,
    Ip,
    Ipv4,
    Ipv6,
    Latitude,
    Longitude,
    Numeric,
    Record,
    Semver,
    Ulid,
    Url,
    Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CryptoDigest {
    Blake3,
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DurationUnit {
    Years,
    Weeks,
    Days,
    Hours,
    Minutes,
    Seconds,
    Milliseconds,
    Microseconds,
    Nanoseconds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimePart {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
    Nano,
    Unix,
    Millis,
    Micros,
    Weekday,
    Week,
    YearDay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimeTruncate {
    Floor,
    Ceil,
    Round,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TypeCast {
    Array,
    Bool,
    Bytes,
    Datetime,
    Decimal,
    Duration,
    File,
    Float,
    Int,
    Number,
    Range,
    Record,
    String,
    StringLossy,
    Table,
    Thing,
    Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TypeKind {
    Array,
    Bool,
    Bytes,
    Collection,
    Datetime,
    Decimal,
    Duration,
    Float,
    None,
    Null,
    Number,
    Object,
    Range,
    Record,
    String,
    Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MathConstant {
    E,
    Frac1Pi,
    Frac1Sqrt2,
    Frac2Pi,
    Frac2SqrtPi,
    FracPi2,
    FracPi3,
    FracPi4,
    FracPi6,
    FracPi8,
    Ln2,
    Ln10,
    Log2E,
    Log2Ten,
    Log10E,
    Log10Two,
    Pi,
    Sqrt2,
    Tau,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MathUnary {
    Abs,
    Acos,
    Acot,
    Asin,
    Atan,
    Ceil,
    Cos,
    Cot,
    DegToRad,
    Floor,
    Ln,
    Log10,
    Log2,
    RadToDeg,
    Round,
    Sign,
    Sin,
    Sqrt,
    Tan,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BuiltinSpec {
    pub(crate) function: Builtin,
    pub(crate) name: &'static str,
    pub(crate) min_arity: usize,
    pub(crate) max_arity: usize,
    pub(crate) class: BuiltinClass,
    pub(crate) syntax: BuiltinSyntax,
    pub(crate) implementation_version: u16,
}

macro_rules! pure {
    ($function:ident, $name:literal, $arity:expr) => {
        BuiltinSpec {
            function: Builtin::$function,
            name: $name,
            min_arity: $arity,
            max_arity: $arity,
            class: BuiltinClass::Pure,
            syntax: BuiltinSyntax::Function,
            implementation_version: 1,
        }
    };
    ($function:ident, $name:literal, $min:expr, $max:expr) => {
        BuiltinSpec {
            function: Builtin::$function,
            name: $name,
            min_arity: $min,
            max_arity: $max,
            class: BuiltinClass::Pure,
            syntax: BuiltinSyntax::Function,
            implementation_version: 1,
        }
    };
}

macro_rules! pure_value {
    ($function:expr, $name:literal, $arity:expr) => {
        BuiltinSpec {
            function: $function,
            name: $name,
            min_arity: $arity,
            max_arity: $arity,
            class: BuiltinClass::Pure,
            syntax: BuiltinSyntax::Function,
            implementation_version: 1,
        }
    };
    ($function:expr, $name:literal, $min:expr, $max:expr) => {
        BuiltinSpec {
            function: $function,
            name: $name,
            min_arity: $min,
            max_arity: $max,
            class: BuiltinClass::Pure,
            syntax: BuiltinSyntax::Function,
            implementation_version: 1,
        }
    };
}

macro_rules! constant {
    ($function:expr, $name:literal) => {
        BuiltinSpec {
            function: $function,
            name: $name,
            min_arity: 0,
            max_arity: 0,
            class: BuiltinClass::Pure,
            syntax: BuiltinSyntax::Constant,
            implementation_version: 1,
        }
    };
}

macro_rules! context {
    ($function:ident, $name:literal, $arity:expr) => {
        BuiltinSpec {
            function: Builtin::$function,
            name: $name,
            min_arity: $arity,
            max_arity: $arity,
            class: BuiltinClass::Context,
            syntax: BuiltinSyntax::Function,
            implementation_version: 1,
        }
    };
}

macro_rules! context_value {
    ($function:expr, $name:literal, $arity:expr) => {
        BuiltinSpec {
            function: $function,
            name: $name,
            min_arity: $arity,
            max_arity: $arity,
            class: BuiltinClass::Context,
            syntax: BuiltinSyntax::Function,
            implementation_version: 1,
        }
    };
    ($function:expr, $name:literal, $min:expr, $max:expr) => {
        BuiltinSpec {
            function: $function,
            name: $name,
            min_arity: $min,
            max_arity: $max,
            class: BuiltinClass::Context,
            syntax: BuiltinSyntax::Function,
            implementation_version: 1,
        }
    };
}

pub(crate) const SPECS: &[BuiltinSpec] = &[
    pure!(ArrayAdd, "array::add", 2),
    pure!(ArrayAppend, "array::append", 2),
    pure!(ArrayAppend, "array::push", 2),
    pure!(ArrayAt, "array::at", 2),
    pure!(ArrayComplement, "array::complement", 2),
    pure!(ArrayConcat, "array::concat", 1, 64),
    pure!(ArrayDifference, "array::difference", 2),
    pure!(ArrayDistinct, "array::distinct", 1),
    pure!(ArrayFirst, "array::first", 1),
    pure!(ArrayIncludes, "array::includes", 2),
    pure!(ArrayIndexOf, "array::index_of", 2),
    pure!(ArrayIntersect, "array::intersect", 2),
    pure!(ArrayIsEmpty, "array::is_empty", 1),
    pure!(ArrayJoin, "array::join", 2),
    pure!(ArrayLast, "array::last", 1),
    pure!(ArrayLen, "array::len", 1),
    pure!(ArrayMax, "array::max", 1),
    pure!(ArrayMin, "array::min", 1),
    pure!(ArrayPop, "array::pop", 1),
    pure!(ArrayPrepend, "array::prepend", 2),
    pure!(ArrayRange, "array::range", 2, 3),
    pure!(ArrayRemove, "array::remove", 2),
    pure!(ArrayRepeat, "array::repeat", 2),
    pure!(ArrayReverse, "array::reverse", 1),
    pure!(ArraySlice, "array::slice", 2, 3),
    pure!(ArraySort, "array::sort", 1, 2),
    pure!(ArraySortAsc, "array::sort::asc", 1),
    pure!(ArraySortDesc, "array::sort::desc", 1),
    pure!(ArrayUnion, "array::union", 2),
    pure_value!(
        Builtin::ArrayBoolean(ArrayBoolean::And),
        "array::boolean_and",
        2
    ),
    pure_value!(
        Builtin::ArrayBoolean(ArrayBoolean::Not),
        "array::boolean_not",
        1
    ),
    pure_value!(
        Builtin::ArrayBoolean(ArrayBoolean::Or),
        "array::boolean_or",
        2
    ),
    pure_value!(
        Builtin::ArrayBoolean(ArrayBoolean::Xor),
        "array::boolean_xor",
        2
    ),
    pure!(ArrayClump, "array::clump", 2),
    pure!(ArrayCombine, "array::combine", 2),
    pure!(ArrayFill, "array::fill", 2, 4),
    pure!(ArrayFlatten, "array::flatten", 1),
    pure!(ArrayGroup, "array::group", 1),
    pure!(ArrayInsert, "array::insert", 2, 3),
    pure_value!(
        Builtin::ArrayLogical(ArrayLogical::And),
        "array::logical_and",
        2
    ),
    pure_value!(
        Builtin::ArrayLogical(ArrayLogical::Or),
        "array::logical_or",
        2
    ),
    pure_value!(
        Builtin::ArrayLogical(ArrayLogical::Xor),
        "array::logical_xor",
        2
    ),
    pure!(ArrayMatches, "array::matches", 2),
    pure!(ArraySequence, "array::sequence", 1, 2),
    context_value!(Builtin::ArrayShuffle, "array::shuffle", 1),
    pure!(ArraySortLexical, "array::sort_lexical", 1),
    pure_value!(Builtin::ArraySortNatural(false), "array::sort_natural", 1),
    pure_value!(
        Builtin::ArraySortNatural(true),
        "array::sort_natural_lexical",
        1
    ),
    pure!(ArraySwap, "array::swap", 3),
    pure!(ArrayTranspose, "array::transpose", 1),
    pure!(ArrayWindows, "array::windows", 2),
    pure_value!(
        Builtin::CollectionClosure(CollectionKind::Array, ClosureOperation::All),
        "array::all",
        2
    ),
    pure_value!(
        Builtin::CollectionClosure(CollectionKind::Array, ClosureOperation::Any),
        "array::any",
        2
    ),
    pure_value!(
        Builtin::CollectionClosure(CollectionKind::Array, ClosureOperation::Filter),
        "array::filter",
        2
    ),
    pure_value!(
        Builtin::CollectionClosure(CollectionKind::Array, ClosureOperation::FilterIndex),
        "array::filter_index",
        2
    ),
    pure_value!(
        Builtin::CollectionClosure(CollectionKind::Array, ClosureOperation::Find),
        "array::find",
        2
    ),
    pure_value!(
        Builtin::CollectionClosure(CollectionKind::Array, ClosureOperation::FindIndex),
        "array::find_index",
        2
    ),
    pure_value!(
        Builtin::CollectionClosure(CollectionKind::Array, ClosureOperation::Fold),
        "array::fold",
        3
    ),
    pure_value!(
        Builtin::CollectionClosure(CollectionKind::Array, ClosureOperation::Map),
        "array::map",
        2
    ),
    pure_value!(
        Builtin::CollectionClosure(CollectionKind::Array, ClosureOperation::Reduce),
        "array::reduce",
        2
    ),
    pure!(BytesLen, "bytes::len", 1),
    pure!(ObjectEntries, "object::entries", 1),
    pure!(ObjectExtend, "object::extend", 2),
    pure!(ObjectFromEntries, "object::from_entries", 1),
    pure!(ObjectIsEmpty, "object::is_empty", 1),
    pure!(ObjectKeys, "object::keys", 1),
    pure!(ObjectLen, "object::len", 1),
    pure!(ObjectRemove, "object::remove", 2, 64),
    pure!(ObjectValues, "object::values", 1),
    pure!(SetAdd, "set::add", 2),
    pure!(SetAt, "set::at", 2),
    pure!(SetComplement, "set::complement", 2),
    pure!(SetContains, "set::contains", 2),
    pure!(SetDifference, "set::difference", 2),
    pure!(SetFirst, "set::first", 1),
    pure!(SetIntersect, "set::intersect", 2),
    pure!(SetIsEmpty, "set::is_empty", 1),
    pure!(SetJoin, "set::join", 2),
    pure!(SetLast, "set::last", 1),
    pure!(SetLen, "set::len", 1),
    pure!(SetMax, "set::max", 1),
    pure!(SetMin, "set::min", 1),
    pure!(SetRemove, "set::remove", 2),
    pure!(SetSlice, "set::slice", 2, 3),
    pure!(SetUnion, "set::union", 2),
    pure!(SetFlatten, "set::flatten", 1),
    pure_value!(
        Builtin::CollectionClosure(CollectionKind::Set, ClosureOperation::All),
        "set::all",
        2
    ),
    pure_value!(
        Builtin::CollectionClosure(CollectionKind::Set, ClosureOperation::Any),
        "set::any",
        2
    ),
    pure_value!(
        Builtin::CollectionClosure(CollectionKind::Set, ClosureOperation::Filter),
        "set::filter",
        2
    ),
    pure_value!(
        Builtin::CollectionClosure(CollectionKind::Set, ClosureOperation::Find),
        "set::find",
        2
    ),
    pure_value!(
        Builtin::CollectionClosure(CollectionKind::Set, ClosureOperation::Fold),
        "set::fold",
        3
    ),
    pure_value!(
        Builtin::CollectionClosure(CollectionKind::Set, ClosureOperation::Map),
        "set::map",
        2
    ),
    pure_value!(
        Builtin::CollectionClosure(CollectionKind::Set, ClosureOperation::Reduce),
        "set::reduce",
        2
    ),
    constant!(Builtin::MathConstant(MathConstant::E), "math::e"),
    constant!(
        Builtin::MathConstant(MathConstant::Frac1Pi),
        "math::frac_1_pi"
    ),
    constant!(
        Builtin::MathConstant(MathConstant::Frac1Sqrt2),
        "math::frac_1_sqrt_2"
    ),
    constant!(
        Builtin::MathConstant(MathConstant::Frac2Pi),
        "math::frac_2_pi"
    ),
    constant!(
        Builtin::MathConstant(MathConstant::Frac2SqrtPi),
        "math::frac_2_sqrt_pi"
    ),
    constant!(
        Builtin::MathConstant(MathConstant::FracPi2),
        "math::frac_pi_2"
    ),
    constant!(
        Builtin::MathConstant(MathConstant::FracPi3),
        "math::frac_pi_3"
    ),
    constant!(
        Builtin::MathConstant(MathConstant::FracPi4),
        "math::frac_pi_4"
    ),
    constant!(
        Builtin::MathConstant(MathConstant::FracPi6),
        "math::frac_pi_6"
    ),
    constant!(
        Builtin::MathConstant(MathConstant::FracPi8),
        "math::frac_pi_8"
    ),
    constant!(Builtin::MathConstant(MathConstant::Ln2), "math::ln_2"),
    constant!(Builtin::MathConstant(MathConstant::Ln10), "math::ln_10"),
    constant!(Builtin::MathConstant(MathConstant::Log2E), "math::log2_e"),
    constant!(
        Builtin::MathConstant(MathConstant::Log2Ten),
        "math::log2_10"
    ),
    constant!(Builtin::MathConstant(MathConstant::Log10E), "math::log10_e"),
    constant!(
        Builtin::MathConstant(MathConstant::Log10Two),
        "math::log10_2"
    ),
    constant!(Builtin::MathConstant(MathConstant::Pi), "math::pi"),
    constant!(Builtin::MathConstant(MathConstant::Sqrt2), "math::sqrt_2"),
    constant!(Builtin::MathConstant(MathConstant::Tau), "math::tau"),
    pure_value!(Builtin::MathUnary(MathUnary::Abs), "math::abs", 1),
    pure_value!(Builtin::MathUnary(MathUnary::Acos), "math::acos", 1),
    pure_value!(Builtin::MathUnary(MathUnary::Acot), "math::acot", 1),
    pure_value!(Builtin::MathUnary(MathUnary::Asin), "math::asin", 1),
    pure_value!(Builtin::MathUnary(MathUnary::Atan), "math::atan", 1),
    pure_value!(Builtin::MathUnary(MathUnary::Ceil), "math::ceil", 1),
    pure_value!(Builtin::MathUnary(MathUnary::Cos), "math::cos", 1),
    pure_value!(Builtin::MathUnary(MathUnary::Cot), "math::cot", 1),
    pure_value!(Builtin::MathUnary(MathUnary::DegToRad), "math::deg2rad", 1),
    pure_value!(Builtin::MathUnary(MathUnary::Floor), "math::floor", 1),
    pure_value!(Builtin::MathUnary(MathUnary::Ln), "math::ln", 1),
    pure_value!(Builtin::MathUnary(MathUnary::Log10), "math::log10", 1),
    pure_value!(Builtin::MathUnary(MathUnary::Log2), "math::log2", 1),
    pure_value!(Builtin::MathUnary(MathUnary::RadToDeg), "math::rad2deg", 1),
    pure_value!(Builtin::MathUnary(MathUnary::Round), "math::round", 1),
    pure_value!(Builtin::MathUnary(MathUnary::Sign), "math::sign", 1),
    pure_value!(Builtin::MathUnary(MathUnary::Sin), "math::sin", 1),
    pure_value!(Builtin::MathUnary(MathUnary::Sqrt), "math::sqrt", 1),
    pure_value!(Builtin::MathUnary(MathUnary::Tan), "math::tan", 1),
    pure!(MathClamp, "math::clamp", 3),
    pure!(MathLerp, "math::lerp", 3),
    pure!(MathLog, "math::log", 2),
    pure!(MathMax, "math::max", 1),
    pure!(MathMean, "math::mean", 1),
    pure!(MathMin, "math::min", 1),
    pure!(MathPow, "math::pow", 2),
    pure!(MathProduct, "math::product", 1),
    pure!(MathSpread, "math::spread", 1),
    pure!(MathSum, "math::sum", 1),
    pure!(MathBottom, "math::bottom", 2),
    pure!(MathFixed, "math::fixed", 2),
    pure!(MathInterquartile, "math::interquartile", 1),
    pure!(MathLerpAngle, "math::lerpangle", 3),
    pure!(MathMedian, "math::median", 1),
    pure!(MathMidhinge, "math::midhinge", 1),
    pure!(MathMode, "math::mode", 1),
    pure!(MathNearestRank, "math::nearestrank", 2),
    pure!(MathPercentile, "math::percentile", 2),
    pure!(MathStddev, "math::stddev", 1),
    pure!(MathTop, "math::top", 2),
    pure!(MathTrimean, "math::trimean", 1),
    pure!(MathVariance, "math::variance", 1),
    pure_value!(Builtin::TypeCast(TypeCast::Array), "type::array", 1),
    pure_value!(Builtin::TypeCast(TypeCast::Bool), "type::bool", 1),
    pure_value!(Builtin::TypeCast(TypeCast::Bytes), "type::bytes", 1),
    pure_value!(Builtin::TypeCast(TypeCast::Datetime), "type::datetime", 1),
    pure_value!(Builtin::TypeCast(TypeCast::Decimal), "type::decimal", 1),
    pure_value!(Builtin::TypeCast(TypeCast::Duration), "type::duration", 1),
    pure_value!(Builtin::TypeCast(TypeCast::File), "type::file", 1),
    pure_value!(Builtin::TypeCast(TypeCast::Float), "type::float", 1),
    pure_value!(Builtin::TypeCast(TypeCast::Int), "type::int", 1),
    pure_value!(Builtin::TypeCast(TypeCast::Number), "type::number", 1),
    pure_value!(Builtin::TypeCast(TypeCast::Range), "type::range", 1),
    pure_value!(Builtin::TypeCast(TypeCast::Record), "type::record", 1, 2),
    pure_value!(Builtin::TypeCast(TypeCast::String), "type::string", 1),
    pure_value!(
        Builtin::TypeCast(TypeCast::StringLossy),
        "type::string_lossy",
        1
    ),
    pure_value!(Builtin::TypeCast(TypeCast::Table), "type::table", 1),
    pure_value!(Builtin::TypeCast(TypeCast::Thing), "type::thing", 1, 2),
    pure_value!(Builtin::TypeCast(TypeCast::Uuid), "type::uuid", 1),
    pure!(TypeOf, "type::of", 1),
    context!(TypeField, "type::field", 1),
    context!(TypeFields, "type::fields", 1),
    pure!(RecordId, "record::id", 1),
    pure!(RecordId, "meta::id", 1),
    pure!(RecordTable, "record::table", 1),
    pure!(RecordTable, "record::tb", 1),
    pure!(RecordTable, "meta::table", 1),
    pure!(RecordTable, "meta::tb", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Array), "type::is::array", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Array), "type::is_array", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Bool), "type::is::bool", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Bool), "type::is_bool", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Bytes), "type::is::bytes", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Bytes), "type::is_bytes", 1),
    pure_value!(
        Builtin::TypeIs(TypeKind::Collection),
        "type::is::collection",
        1
    ),
    pure_value!(
        Builtin::TypeIs(TypeKind::Collection),
        "type::is_collection",
        1
    ),
    pure_value!(Builtin::TypeIs(TypeKind::Datetime), "type::is::datetime", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Datetime), "type::is_datetime", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Decimal), "type::is::decimal", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Decimal), "type::is_decimal", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Duration), "type::is::duration", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Duration), "type::is_duration", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Float), "type::is::float", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Float), "type::is_float", 1),
    pure_value!(Builtin::TypeIs(TypeKind::None), "type::is::none", 1),
    pure_value!(Builtin::TypeIs(TypeKind::None), "type::is_none", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Null), "type::is::null", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Null), "type::is_null", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Number), "type::is::number", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Number), "type::is_number", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Object), "type::is::object", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Object), "type::is_object", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Range), "type::is::range", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Range), "type::is_range", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Record), "type::is::record", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Record), "type::is_record", 1),
    pure_value!(Builtin::TypeIs(TypeKind::String), "type::is::string", 1),
    pure_value!(Builtin::TypeIs(TypeKind::String), "type::is_string", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Uuid), "type::is::uuid", 1),
    pure_value!(Builtin::TypeIs(TypeKind::Uuid), "type::is_uuid", 1),
    constant!(Builtin::DurationMax, "duration::max"),
    pure_value!(
        Builtin::DurationExtract(DurationUnit::Years),
        "duration::years",
        1
    ),
    pure_value!(
        Builtin::DurationExtract(DurationUnit::Weeks),
        "duration::weeks",
        1
    ),
    pure_value!(
        Builtin::DurationExtract(DurationUnit::Days),
        "duration::days",
        1
    ),
    pure_value!(
        Builtin::DurationExtract(DurationUnit::Hours),
        "duration::hours",
        1
    ),
    pure_value!(
        Builtin::DurationExtract(DurationUnit::Minutes),
        "duration::mins",
        1
    ),
    pure_value!(
        Builtin::DurationExtract(DurationUnit::Seconds),
        "duration::secs",
        1
    ),
    pure_value!(
        Builtin::DurationExtract(DurationUnit::Milliseconds),
        "duration::millis",
        1
    ),
    pure_value!(
        Builtin::DurationExtract(DurationUnit::Microseconds),
        "duration::micros",
        1
    ),
    pure_value!(
        Builtin::DurationExtract(DurationUnit::Nanoseconds),
        "duration::nanos",
        1
    ),
    pure_value!(
        Builtin::DurationFrom(DurationUnit::Weeks),
        "duration::from_weeks",
        1
    ),
    pure_value!(
        Builtin::DurationFrom(DurationUnit::Days),
        "duration::from_days",
        1
    ),
    pure_value!(
        Builtin::DurationFrom(DurationUnit::Hours),
        "duration::from_hours",
        1
    ),
    pure_value!(
        Builtin::DurationFrom(DurationUnit::Minutes),
        "duration::from_mins",
        1
    ),
    pure_value!(
        Builtin::DurationFrom(DurationUnit::Seconds),
        "duration::from_secs",
        1
    ),
    pure_value!(
        Builtin::DurationFrom(DurationUnit::Milliseconds),
        "duration::from_millis",
        1
    ),
    pure_value!(
        Builtin::DurationFrom(DurationUnit::Microseconds),
        "duration::from_micros",
        1
    ),
    pure_value!(
        Builtin::DurationFrom(DurationUnit::Nanoseconds),
        "duration::from_nanos",
        1
    ),
    constant!(Builtin::TimeEpoch, "time::epoch"),
    context!(TimeNow, "time::now", 0),
    context!(TimeTimezone, "time::timezone", 0),
    pure_value!(Builtin::TimePart(TimePart::Year), "time::year", 1),
    pure_value!(Builtin::TimePart(TimePart::Month), "time::month", 1),
    pure_value!(Builtin::TimePart(TimePart::Day), "time::day", 1),
    pure_value!(Builtin::TimePart(TimePart::Hour), "time::hour", 1),
    pure_value!(Builtin::TimePart(TimePart::Minute), "time::minute", 1),
    pure_value!(Builtin::TimePart(TimePart::Second), "time::second", 1),
    pure_value!(Builtin::TimePart(TimePart::Nano), "time::nano", 1),
    pure_value!(Builtin::TimePart(TimePart::Unix), "time::unix", 1),
    pure_value!(Builtin::TimePart(TimePart::Millis), "time::millis", 1),
    pure_value!(Builtin::TimePart(TimePart::Micros), "time::micros", 1),
    pure_value!(Builtin::TimePart(TimePart::Weekday), "time::wday", 1),
    pure_value!(Builtin::TimePart(TimePart::Week), "time::week", 1),
    pure_value!(Builtin::TimePart(TimePart::YearDay), "time::yday", 1),
    pure_value!(
        Builtin::TimeFrom(DurationUnit::Seconds),
        "time::from_secs",
        1
    ),
    pure_value!(
        Builtin::TimeFrom(DurationUnit::Seconds),
        "time::from_unix",
        1
    ),
    pure_value!(
        Builtin::TimeFrom(DurationUnit::Milliseconds),
        "time::from_millis",
        1
    ),
    pure_value!(
        Builtin::TimeFrom(DurationUnit::Microseconds),
        "time::from_micros",
        1
    ),
    pure_value!(
        Builtin::TimeFrom(DurationUnit::Nanoseconds),
        "time::from_nanos",
        1
    ),
    pure!(TimeFromUuid, "time::from_uuid", 1),
    pure!(TimeFromUlid, "time::from_ulid", 1),
    pure!(TimeIsLeapYear, "time::is_leap_year", 1),
    pure!(TimeMin, "time::min", 1),
    pure!(TimeMax, "time::max", 1),
    pure!(TimeFormat, "time::format", 2),
    pure_value!(Builtin::TimeSet(TimePart::Year), "time::set_year", 2),
    pure_value!(Builtin::TimeSet(TimePart::Month), "time::set_month", 2),
    pure_value!(Builtin::TimeSet(TimePart::Day), "time::set_day", 2),
    pure_value!(Builtin::TimeSet(TimePart::Hour), "time::set_hour", 2),
    pure_value!(Builtin::TimeSet(TimePart::Minute), "time::set_minute", 2),
    pure_value!(Builtin::TimeSet(TimePart::Second), "time::set_second", 2),
    pure_value!(Builtin::TimeSet(TimePart::Nano), "time::set_nanosecond", 2),
    pure_value!(Builtin::TimeTruncate(TimeTruncate::Floor), "time::floor", 2),
    pure_value!(Builtin::TimeTruncate(TimeTruncate::Ceil), "time::ceil", 2),
    pure_value!(Builtin::TimeTruncate(TimeTruncate::Round), "time::round", 2),
    pure_value!(Builtin::TimeTruncate(TimeTruncate::Floor), "time::group", 2),
    pure!(EncodingBase64Encode, "encoding::base64::encode", 1),
    pure!(EncodingBase64Decode, "encoding::base64::decode", 1),
    pure!(EncodingJsonEncode, "encoding::json::encode", 1),
    pure!(EncodingJsonDecode, "encoding::json::decode", 1),
    pure!(EncodingCborEncode, "encoding::cbor::encode", 1),
    pure!(EncodingCborDecode, "encoding::cbor::decode", 1),
    pure_value!(
        Builtin::CryptoDigest(CryptoDigest::Blake3),
        "crypto::blake3",
        1
    ),
    pure_value!(Builtin::CryptoDigest(CryptoDigest::Md5), "crypto::md5", 1),
    pure_value!(Builtin::CryptoDigest(CryptoDigest::Sha1), "crypto::sha1", 1),
    pure_value!(
        Builtin::CryptoDigest(CryptoDigest::Sha256),
        "crypto::sha256",
        1
    ),
    pure_value!(
        Builtin::CryptoDigest(CryptoDigest::Sha512),
        "crypto::sha512",
        1
    ),
    pure!(CryptoJoaat, "crypto::joaat", 1),
    pure!(Count, "count", 0, 1),
    pure!(Not, "not", 1),
    pure!(ValueExpect, "value::expect", 2, 3),
    context_value!(
        Builtin::CryptoPassword(PasswordAlgorithm::Argon2, PasswordOperation::Compare),
        "crypto::argon2::compare",
        2
    ),
    context_value!(
        Builtin::CryptoPassword(PasswordAlgorithm::Argon2, PasswordOperation::Generate),
        "crypto::argon2::generate",
        1
    ),
    context_value!(
        Builtin::CryptoPassword(PasswordAlgorithm::Bcrypt, PasswordOperation::Compare),
        "crypto::bcrypt::compare",
        2
    ),
    context_value!(
        Builtin::CryptoPassword(PasswordAlgorithm::Bcrypt, PasswordOperation::Generate),
        "crypto::bcrypt::generate",
        1
    ),
    context_value!(
        Builtin::CryptoPassword(PasswordAlgorithm::Pbkdf2, PasswordOperation::Compare),
        "crypto::pbkdf2::compare",
        2
    ),
    context_value!(
        Builtin::CryptoPassword(PasswordAlgorithm::Pbkdf2, PasswordOperation::Generate),
        "crypto::pbkdf2::generate",
        1
    ),
    context_value!(
        Builtin::CryptoPassword(PasswordAlgorithm::Scrypt, PasswordOperation::Compare),
        "crypto::scrypt::compare",
        2
    ),
    context_value!(
        Builtin::CryptoPassword(PasswordAlgorithm::Scrypt, PasswordOperation::Generate),
        "crypto::scrypt::generate",
        1
    ),
    context_value!(Builtin::Random(RandomBuiltin::Bool), "rand::bool", 0),
    context_value!(
        Builtin::Random(RandomBuiltin::Duration),
        "rand::duration",
        2
    ),
    context_value!(Builtin::Random(RandomBuiltin::Enum), "rand::enum", 1, 64),
    context_value!(Builtin::Random(RandomBuiltin::Float), "rand::float", 0, 2),
    context_value!(Builtin::Random(RandomBuiltin::Id), "rand::id", 0, 1),
    context_value!(Builtin::Random(RandomBuiltin::Int), "rand::int", 0, 2),
    context_value!(Builtin::Random(RandomBuiltin::String), "rand::string", 0, 1),
    context_value!(Builtin::Random(RandomBuiltin::Time), "rand::time", 2),
    context_value!(Builtin::Random(RandomBuiltin::Ulid), "rand::ulid", 0),
    context_value!(Builtin::Random(RandomBuiltin::UuidV7), "rand::uuid", 0),
    context_value!(Builtin::Random(RandomBuiltin::UuidV4), "rand::uuid::v4", 0),
    context_value!(Builtin::Random(RandomBuiltin::UuidV7), "rand::uuid::v7", 0),
    pure_value!(
        Builtin::String(StringBuiltin::ParseEmailHost),
        "parse::email::host",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::ParseEmailUser),
        "parse::email::user",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::ParseUrl(StringUrlPart::Domain)),
        "parse::url::domain",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::ParseUrl(StringUrlPart::Fragment)),
        "parse::url::fragment",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::ParseUrl(StringUrlPart::Host)),
        "parse::url::host",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::ParseUrl(StringUrlPart::Path)),
        "parse::url::path",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::ParseUrl(StringUrlPart::Port)),
        "parse::url::port",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::ParseUrl(StringUrlPart::Query)),
        "parse::url::query",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::ParseUrl(StringUrlPart::Scheme)),
        "parse::url::scheme",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Capitalize),
        "string::capitalize",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Concat),
        "string::concat",
        1,
        64
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Contains),
        "string::contains",
        2
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Distance(StringDistance::DamerauLevenshtein)),
        "string::distance::damerau_levenshtein",
        2
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Distance(StringDistance::Hamming)),
        "string::distance::hamming",
        2
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Distance(StringDistance::Levenshtein)),
        "string::distance::levenshtein",
        2
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Distance(
            StringDistance::NormalizedDamerauLevenshtein
        )),
        "string::distance::normalized_damerau_levenshtein",
        2
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Distance(
            StringDistance::NormalizedLevenshtein
        )),
        "string::distance::normalized_levenshtein",
        2
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Distance(StringDistance::Osa)),
        "string::distance::osa",
        2
    ),
    pure_value!(
        Builtin::String(StringBuiltin::EndsWith),
        "string::ends_with",
        2
    ),
    pure_value!(
        Builtin::String(StringBuiltin::EndsWith),
        "string::endswith",
        2
    ),
    pure_value!(
        Builtin::String(StringBuiltin::HtmlEncode),
        "string::html::encode",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::HtmlSanitize),
        "string::html::sanitize",
        1
    ),
    pure_value!(Builtin::String(StringBuiltin::Join), "string::join", 2, 64),
    pure_value!(Builtin::String(StringBuiltin::Len), "string::len", 1),
    pure_value!(
        Builtin::String(StringBuiltin::Lowercase),
        "string::lowercase",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Matches),
        "string::matches",
        2
    ),
    pure_value!(Builtin::String(StringBuiltin::Repeat), "string::repeat", 2),
    pure_value!(
        Builtin::String(StringBuiltin::Replace),
        "string::replace",
        3
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Reverse),
        "string::reverse",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Semver(StringSemver::Compare)),
        "string::semver::compare",
        2
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Semver(StringSemver::IncMajor)),
        "string::semver::inc::major",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Semver(StringSemver::IncMinor)),
        "string::semver::inc::minor",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Semver(StringSemver::IncPatch)),
        "string::semver::inc::patch",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Semver(StringSemver::Major)),
        "string::semver::major",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Semver(StringSemver::Minor)),
        "string::semver::minor",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Semver(StringSemver::Patch)),
        "string::semver::patch",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Semver(StringSemver::SetMajor)),
        "string::semver::set::major",
        2
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Semver(StringSemver::SetMinor)),
        "string::semver::set::minor",
        2
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Semver(StringSemver::SetPatch)),
        "string::semver::set::patch",
        2
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Similarity(StringSimilarity::Fuzzy)),
        "string::similarity::fuzzy",
        2
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Similarity(StringSimilarity::Jaro)),
        "string::similarity::jaro",
        2
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Similarity(StringSimilarity::JaroWinkler)),
        "string::similarity::jaro_winkler",
        2
    ),
    pure_value!(Builtin::String(StringBuiltin::Slice), "string::slice", 2, 3),
    pure_value!(Builtin::String(StringBuiltin::Slug), "string::slug", 1),
    pure_value!(Builtin::String(StringBuiltin::Split), "string::split", 2),
    pure_value!(
        Builtin::String(StringBuiltin::StartsWith),
        "string::starts_with",
        2
    ),
    pure_value!(
        Builtin::String(StringBuiltin::StartsWith),
        "string::startswith",
        2
    ),
    pure_value!(Builtin::String(StringBuiltin::Trim), "string::trim", 1),
    pure_value!(
        Builtin::String(StringBuiltin::Uppercase),
        "string::uppercase",
        1
    ),
    pure_value!(Builtin::String(StringBuiltin::Words), "string::words", 1),
    pure_value!(
        Builtin::String(StringBuiltin::Validate(StringValidator::Alpha)),
        "string::is_alpha",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Validate(StringValidator::Alphanumeric)),
        "string::is_alphanum",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Validate(StringValidator::Ascii)),
        "string::is_ascii",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Validate(StringValidator::Datetime)),
        "string::is_datetime",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Validate(StringValidator::Domain)),
        "string::is_domain",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Validate(StringValidator::Email)),
        "string::is_email",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Validate(StringValidator::Hexadecimal)),
        "string::is_hexadecimal",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Validate(StringValidator::Ip)),
        "string::is_ip",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Validate(StringValidator::Ipv4)),
        "string::is_ipv4",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Validate(StringValidator::Ipv6)),
        "string::is_ipv6",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Validate(StringValidator::Latitude)),
        "string::is_latitude",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Validate(StringValidator::Longitude)),
        "string::is_longitude",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Validate(StringValidator::Numeric)),
        "string::is_numeric",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Validate(StringValidator::Record)),
        "string::is_record",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Validate(StringValidator::Semver)),
        "string::is_semver",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Validate(StringValidator::Ulid)),
        "string::is_ulid",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Validate(StringValidator::Url)),
        "string::is_url",
        1
    ),
    pure_value!(
        Builtin::String(StringBuiltin::Validate(StringValidator::Uuid)),
        "string::is_uuid",
        1
    ),
];

pub(crate) fn lookup(name: &str) -> Option<&'static BuiltinSpec> {
    SPECS.iter().find(|spec| spec.name == name)
}

#[cfg(test)]
mod tests {
    use super::{lookup, BuiltinClass, BuiltinSyntax, SPECS};
    use std::collections::BTreeSet;

    #[test]
    fn p13_fn_001_registry_names_and_metadata_are_closed_and_unique() {
        let mut names = BTreeSet::new();
        for spec in SPECS {
            assert!(names.insert(spec.name));
            assert!(spec.min_arity <= spec.max_arity);
            assert!(matches!(
                spec.class,
                BuiltinClass::Pure | BuiltinClass::Context
            ));
            assert!(matches!(
                spec.syntax,
                BuiltinSyntax::Function | BuiltinSyntax::Constant
            ));
            assert_eq!(spec.implementation_version, 1);
            assert_eq!(lookup(spec.name).unwrap().function, spec.function);
        }

        assert!(lookup("array::unknown").is_none());
    }
}
