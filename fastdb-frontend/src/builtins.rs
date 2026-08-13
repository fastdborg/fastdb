//! Closed metadata registry for FastDB-owned built-in functions.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuiltinClass {
    Pure,
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
            assert_eq!(spec.class, BuiltinClass::Pure);
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
