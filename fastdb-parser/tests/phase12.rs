#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb_parser::{parse, SchemaTypeKind, Statement};

#[test]
fn p12_parse_001_non_geospatial_types_and_typed_collections_are_structured() {
    let source = "\
        DEFINE FIELD blob ON item TYPE bytes;\
        DEFINE FIELD created ON item TYPE datetime;\
        DEFINE FIELD amount ON item TYPE decimal;\
        DEFINE FIELD elapsed ON item TYPE duration;\
        DEFINE FIELD file_ref ON item TYPE file;\
        DEFINE FIELD pattern ON item TYPE regex;\
        DEFINE FIELD span ON item TYPE range;\
        DEFINE FIELD tags ON item TYPE set<string, 2>;\
        DEFINE FIELD owner_table ON item TYPE table;\
        DEFINE FIELD pair ON item TYPE array<int, 2>;\
        DEFINE FIELD uuid_value ON item TYPE uuid";
    let statements = parse(source).unwrap();
    assert_eq!(statements.statements.len(), 11);
    let Statement::DefineField(tags) = &statements.statements[7] else {
        panic!("expected field definition")
    };
    assert!(matches!(
        &tags.ty.kind,
        SchemaTypeKind::Set {
            element: Some(element),
            length: Some(length),
        } if matches!(element.kind, SchemaTypeKind::String) && length.value == 2
    ));
    let Statement::DefineField(pair) = &statements.statements[9] else {
        panic!("expected field definition")
    };
    assert!(matches!(
        &pair.ty.kind,
        SchemaTypeKind::TypedArray {
            element,
            length: Some(length),
        } if matches!(element.kind, SchemaTypeKind::Int) && length.value == 2
    ));
}

#[test]
fn p12_parse_002_typed_collection_limits_fail_during_parsing() {
    for source in [
        "DEFINE FIELD xs ON item TYPE array<int, 0>",
        "DEFINE FIELD xs ON item TYPE set<string, 65537>",
    ] {
        assert!(parse(source).is_err(), "accepted {source}");
    }
}
