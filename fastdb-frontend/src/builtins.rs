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
    TypeCast(TypeCast),
    TypeIs(TypeKind),
    TypeOf,
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
