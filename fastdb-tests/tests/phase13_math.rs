#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb::{Database, StatementResult, Value};

fn row(result: &StatementResult) -> &std::collections::BTreeMap<String, Value> {
    let StatementResult::Rows(rows) = result else {
        panic!("expected rows")
    };
    let Value::Object(row) = &rows[0] else {
        panic!("expected object row")
    };
    row
}

fn float(row: &std::collections::BTreeMap<String, Value>, key: &str) -> f64 {
    let Some(Value::Float(value)) = row.get(key) else {
        panic!("expected float field {key}")
    };
    *value
}

fn assert_close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() <= 1e-12, "{actual} != {expected}");
}

#[test]
fn p13_fn_005_math_constants_scalars_and_aggregates_execute() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE math_result:one SET \
             e_value = math::e, frac_1_pi = math::frac_1_pi, \
             frac_1_sqrt_2 = math::frac_1_sqrt_2, frac_2_pi = math::frac_2_pi, \
             frac_2_sqrt_pi = math::frac_2_sqrt_pi, frac_pi_2 = math::frac_pi_2, \
             frac_pi_3 = math::frac_pi_3, frac_pi_4 = math::frac_pi_4, \
             frac_pi_6 = math::frac_pi_6, frac_pi_8 = math::frac_pi_8, \
             ln_2 = math::ln_2, ln_10 = math::ln_10, log2_e = math::log2_e, \
             log2_10 = math::log2_10, log10_e = math::log10_e, \
             log10_2 = math::log10_2, pi_value = math::pi, sqrt_2 = math::sqrt_2, \
             tau_value = math::tau, absolute = math::abs(-2), acos_value = math::acos(1), \
             acot_value = math::acot(1), asin_value = math::asin(0), \
             atan_value = math::atan(1), ceil_value = math::ceil(1.2), \
             cos_value = math::cos(0), cot_value = math::cot(1), \
             radians = math::deg2rad(180), floor_value = math::floor(1.8), \
             ln_value = math::ln(1), log10_value = math::log10(1), \
             log2_value = math::log2(1), degrees = math::rad2deg(math::pi), \
             rounded = math::round(1.5), sign_value = math::sign(-2), \
             sin_value = math::sin(0), square_root = math::sqrt(4), \
             tan_value = math::tan(0), clamped = math::clamp(5,1,3), \
             lerped = math::lerp(0,10,0.25), logged = math::log(8,2), \
             maximum = math::max([1,2.5]), mean_value = math::mean([1,2]), \
             minimum = math::min([1,2]), powered = math::pow(2,3), \
             product_value = math::product([2,3]), spread_value = math::spread([1,4]), \
             sum_value = math::sum([1,2,3])",
        )
        .unwrap();
    let selected = connection.execute("SELECT * FROM math_result:one").unwrap();
    let row = row(&selected.statements[0]);

    assert_close(float(row, "e_value"), std::f64::consts::E);
    assert_close(float(row, "pi_value"), std::f64::consts::PI);
    assert_close(float(row, "tau_value"), std::f64::consts::TAU);
    assert_close(float(row, "radians"), std::f64::consts::PI);
    assert_close(float(row, "degrees"), 180.0);
    assert_close(float(row, "lerped"), 2.5);
    assert_close(float(row, "logged"), 3.0);
    assert_close(float(row, "mean_value"), 1.5);
    assert_eq!(row.get("absolute"), Some(&Value::Integer(2)));
    assert_eq!(row.get("clamped"), Some(&Value::Integer(3)));
    assert_eq!(row.get("powered"), Some(&Value::Integer(8)));
    assert_eq!(row.get("product_value"), Some(&Value::Integer(6)));
    assert_eq!(row.get("spread_value"), Some(&Value::Integer(3)));
    assert_eq!(row.get("sum_value"), Some(&Value::Integer(6)));
    assert_eq!(row.get("sign_value"), Some(&Value::Integer(-1)));
    assert_eq!(row.len(), 49);
}

#[test]
fn p13_fn_006_math_domain_and_constant_call_errors_are_atomic() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    for source in [
        "CREATE bad:one SET value = math::sqrt(-1)",
        "CREATE bad:one SET value = math::pi()",
        "CREATE bad:one SET value = math::clamp(1,3,2)",
    ] {
        assert!(connection.execute(source).is_err(), "accepted {source}");
    }
    let result = connection.execute("SELECT * FROM bad").unwrap();
    let StatementResult::Rows(rows) = &result.statements[0] else {
        panic!("expected rows")
    };
    assert!(rows.is_empty());
}
