#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb_parser::{
    parse_one, Accessor, BinaryOperator, CreateData, Expr, ExprKind, SchemaTypeKind, Statement,
};

fn assignments(source: &str) -> Vec<Expr> {
    let Statement::Create(statement) = parse_one(source).unwrap() else {
        panic!("expected CREATE")
    };
    let CreateData::Set(assignments) = statement.data else {
        panic!("expected SET")
    };
    assignments
        .into_iter()
        .map(|assignment| assignment.value)
        .collect()
}

#[test]
fn p13_parse_001_operators_are_structured_with_reference_binding_power() {
    let values = assignments(
        "CREATE calc:one SET \
         coalesce = NONE ?? 1 + 2, \
         power = -2 ** 3 ** 2, \
         contain_result = [1,2] CONTAINSANY [2,3] AND 2 IN [1,2], \
         contain_all = [1,2] CONTAINSALL [1,2], contain_none = [1] CONTAINSNONE [2], \
         any_inside = [1] ANYINSIDE [1,2], all_inside = [1] ALLINSIDE [1,2], \
         exact = 1 == 1.0, any = [1,2] ?= 2, all = [2,2] *= 2, \
         aliases = [1] ∋ 1 AND 1 ∈ [1] AND [1] ⊆ [1,2]",
    );

    let ExprKind::Binary {
        left,
        operator,
        right,
    } = &values[0].kind
    else {
        panic!("expected outer null coalescing expression")
    };
    assert_eq!(operator.value, BinaryOperator::NullCoalesce);
    assert!(matches!(left.kind, ExprKind::None));
    assert!(matches!(
        right.kind,
        ExprKind::Binary {
            operator: ref nested,
            ..
        } if nested.value == BinaryOperator::Add
    ));

    assert!(matches!(
        values[1].kind,
        ExprKind::Binary {
            operator: ref outer,
            left: ref power_left,
            ..
        } if outer.value == BinaryOperator::Power
            && matches!(power_left.kind, ExprKind::Binary { operator: ref inner, .. }
                if inner.value == BinaryOperator::Power)
    ));
    for value in &values[2..] {
        assert!(matches!(value.kind, ExprKind::Binary { .. }));
    }
}

#[test]
fn p13_parse_002_index_slice_cast_range_and_none_are_explicit() {
    let values = assignments(
        "CREATE calc:one SET \
         first = [1,2,3][0], last = [1,2,3][$], \
         middle = [1,2,3][1..=2], open = [1,2,3][..2], \
         casted = <array<int, 2>> ['1','2'], span = 1..=3, absent = NONE",
    );
    assert!(matches!(
        values[0].kind,
        ExprKind::Access {
            accessor: Accessor::Index(_),
            ..
        }
    ));
    assert!(matches!(
        values[1].kind,
        ExprKind::Access {
            accessor: Accessor::Last(_),
            ..
        }
    ));
    assert!(matches!(
        values[2].kind,
        ExprKind::Access {
            accessor: Accessor::Slice {
                start: Some(_),
                end: Some(_),
                inclusive: true,
                ..
            },
            ..
        }
    ));
    assert!(matches!(
        values[3].kind,
        ExprKind::Access {
            accessor: Accessor::Slice {
                start: None,
                end: Some(_),
                inclusive: false,
                ..
            },
            ..
        }
    ));
    assert!(matches!(
        values[4].kind,
        ExprKind::Cast {
            ty: ref cast,
            ..
        } if matches!(cast.kind, SchemaTypeKind::TypedArray { .. })
    ));
    assert!(matches!(
        values[5].kind,
        ExprKind::Range(ref range) if range.inclusive
    ));
    assert!(matches!(values[6].kind, ExprKind::None));
}

#[test]
fn p13_parse_003_compound_and_symbolic_forms_share_operators() {
    let values = assignments(
        "CREATE calc:one SET \
         a = 1 IS 1, b = 1 IS NOT 2, c = 1 NOT IN [2], \
         d = true && false || true, e = !!1, f = 5 % 2, \
         g = [1] CONTAINSNOT 2, h = [1] NONEINSIDE [2]",
    );
    let expected = [
        BinaryOperator::Equal,
        BinaryOperator::NotEqual,
        BinaryOperator::NotInside,
        BinaryOperator::Or,
    ];
    for (value, expected) in values.iter().zip(expected) {
        assert!(matches!(
            value.kind,
            ExprKind::Binary { operator: ref actual, .. } if actual.value == expected
        ));
    }
    assert!(matches!(values[4].kind, ExprKind::Unary { .. }));
    assert!(matches!(
        values[5].kind,
        ExprKind::Binary { operator: ref op, .. } if op.value == BinaryOperator::Modulo
    ));
}
