//! Two-dimensional point functions, H3 cells and managed radius-search indexes.
use crate::{Document, Error, Result, Value};

const EARTH_RADIUS_M: f64 = 6_371_008.8;

fn invalid(message: &str) -> Error {
    Error::Validation(format!("geo: {message}"))
}

fn number(value: &Value) -> Result<f64> {
    let value = match value {
        Value::Integer(n) => *n as f64,
        Value::Number(n) => *n,
        _ => return Err(invalid("expected finite number")),
    };
    if !value.is_finite() {
        return Err(invalid("expected finite number"));
    }
    Ok(value)
}

fn coordinates(longitude: &Value, latitude: &Value) -> Result<(f64, f64)> {
    let longitude = number(longitude)?;
    let latitude = number(latitude)?;
    if !(-180.0..=180.0).contains(&longitude) || !(-90.0..=90.0).contains(&latitude) {
        return Err(invalid(
            "longitude must be in [-180,180] and latitude in [-90,90]",
        ));
    }
    Ok((longitude, latitude))
}

fn point(value: &Value) -> Result<(f64, f64)> {
    let Value::Object(fields) = value else {
        return Err(invalid("expected Point object"));
    };
    if fields.len() != 2
        || !matches!(fields.get("type"), Some(Value::String(kind)) if kind == "Point")
    {
        return Err(invalid("expected only Point type and coordinates"));
    }
    let Some(Value::Array(values)) = fields.get("coordinates") else {
        return Err(invalid("expected coordinate array"));
    };
    let [longitude, latitude] = values.as_slice() else {
        return Err(invalid("expected two coordinates"));
    };
    coordinates(longitude, latitude)
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    // Normalize the difference, not stored values. Preserve exact zero for
    // equivalent antimeridian coordinates and coincident poles.
    let longitude = b.0 - a.0;
    let longitude = if longitude > 180.0 {
        longitude - 360.0
    } else if longitude < -180.0 {
        longitude + 360.0
    } else {
        longitude
    };
    if a.1 == b.1 && (longitude == 0.0 || a.1.abs() == 90.0) {
        return 0.0;
    }
    let latitude = (b.1 - a.1).to_radians();
    let cosine = |lat: f64| {
        if lat.abs() == 90.0 {
            0.0
        } else {
            lat.to_radians().cos()
        }
    };
    let h = (latitude / 2.0).sin().powi(2)
        + cosine(a.1) * cosine(b.1) * (longitude.to_radians() / 2.0).sin().powi(2);
    let h = h.clamp(0.0, 1.0);
    2.0 * EARTH_RADIUS_M * h.sqrt().atan2((1.0 - h).sqrt())
}

pub(crate) fn call(name: &str, args: &[Value]) -> Result<Value> {
    match (name, args) {
        ("point", [longitude, latitude]) => {
            let (longitude, latitude) = coordinates(longitude, latitude)?;
            Ok(Value::Object(Document::from([
                ("type".into(), Value::String("Point".into())),
                (
                    "coordinates".into(),
                    Value::Array(vec![Value::Number(longitude), Value::Number(latitude)]),
                ),
            ])))
        }
        ("cell", [value, resolution]) => {
            let (mut longitude, latitude) = point(value)?;
            // Equivalent geographic positions must get the same cell even at
            // fine resolutions near poles and the antimeridian.
            if latitude.abs() == 90.0 {
                longitude = 0.0;
            }
            if longitude == -180.0 {
                longitude = 180.0;
            }
            let resolution = number(resolution)?;
            if !(0.0..=15.0).contains(&resolution) || resolution.fract() != 0.0 {
                return Err(invalid(
                    "cell resolution must be an integer from 0 through 15",
                ));
            }
            let resolution = h3o::Resolution::try_from(resolution as u8)
                .map_err(|_| invalid("invalid cell resolution"))?;
            let coordinate = h3o::LatLng::new(latitude, longitude)
                .map_err(|_| invalid("invalid cell coordinates"))?;
            Ok(Value::String(coordinate.to_cell(resolution).to_string()))
        }
        ("cell_center", [Value::String(address)]) => {
            let cell = address
                .parse::<h3o::CellIndex>()
                .map_err(|_| invalid("invalid H3 cell address"))?;
            if cell.to_string() != *address {
                return Err(invalid(
                    "cell address must use canonical lowercase hexadecimal",
                ));
            }
            let center = h3o::LatLng::from(cell);
            call(
                "point",
                &[Value::Number(center.lng()), Value::Number(center.lat())],
            )
        }
        ("distance", [a, b]) => Ok(Value::Number(distance(point(a)?, point(b)?))),
        ("within", [a, b, radius]) => {
            let a = point(a)?;
            let b = point(b)?;
            let radius = number(radius)?;
            if radius < 0.0 {
                return Err(invalid("radius must be nonnegative"));
            }
            Ok(Value::Boolean(distance(a, b) <= radius))
        }
        _ => Err(invalid("invalid function or argument count")),
    }
}

impl crate::Index {
    pub(crate) fn table_ddl(&self) -> String {
        if self.kind == crate::IndexKind::FullText {
            return self.text_table_ddl();
        }
        let extra = if self.kind == crate::IndexKind::Spatial {
            ", longitude"
        } else {
            ""
        };
        format!(
            "CREATE TABLE {} (\"key\", id BLOB NOT NULL{extra})",
            crate::quote(&self.storage)
        )
    }
    pub(crate) fn index_ddl(&self) -> String {
        if self.kind == crate::IndexKind::Vector {
            return format!(
                "CREATE UNIQUE INDEX {} ON {} (id)",
                crate::quote(&self.name),
                crate::quote(&self.storage)
            );
        }
        if self.kind == crate::IndexKind::FullText {
            return self.text_index_ddl();
        }
        // Spatial entries include nulls for optional fields. Radius predicates
        // exclude them while integrity audits retain one entry per document.
        format!(
            "CREATE {} INDEX {} ON {} (\"key\")",
            if self.unique { "UNIQUE" } else { "" },
            crate::quote(&self.name),
            crate::quote(&self.storage)
        )
    }
    pub(crate) fn keys(&self, value: &Value) -> Result<Vec<turso_core::Value>> {
        if self.kind == crate::IndexKind::Vector {
            return self.vector_keys(value);
        }
        if self.kind == crate::IndexKind::Scalar {
            return Ok(vec![crate::index_scalar(value)?]);
        }
        if matches!(value, Value::Null) {
            return Ok(vec![turso_core::Value::Null, turso_core::Value::Null]);
        }
        let (longitude, latitude) = point(value)?;
        Ok(vec![
            crate::scalar(&Value::Number(latitude))?,
            crate::scalar(&Value::Number(longitude))?,
        ])
    }
}

/// Evaluate only immutable inputs, once per preparation; never execute SQL or callbacks.
pub(crate) fn search_argument(
    expr: &turso_parser::ast::Expr,
    params: &crate::Parameters,
    consumed: &mut std::collections::BTreeSet<String>,
) -> Result<Value> {
    use turso_parser::ast::{Expr, Literal, UnaryOperator};
    match expr {
        Expr::Literal(Literal::Numeric(n)) => {
            let number = n
                .parse::<f64>()
                .map_err(|_| invalid("expected decimal number"))?;
            if !number.is_finite() {
                return Err(invalid("expected finite number"));
            }
            Ok(Value::Number(number))
        }
        Expr::Literal(Literal::String(s)) => {
            let tokens = fastql_parser::tokenize(s)?;
            match tokens.as_slice() {
                [token] if token.kind == fastql_parser::Kind::String => {
                    Ok(Value::String(token.text.clone()))
                }
                _ => Err(invalid("expected string literal")),
            }
        }
        Expr::Variable(v) => {
            let name = v
                .name
                .as_ref()
                .map_or_else(|| format!("?{}", v.index), |name| name.to_string());
            consumed.insert(name.clone());
            let value = params
                .get(&name)
                .cloned()
                .ok_or(crate::Error::Parameter(name))?;
            value.validate()?;
            Ok(value)
        }
        Expr::Parenthesized(es) if es.len() == 1 => search_argument(&es[0], params, consumed),
        Expr::Unary(op @ (UnaryOperator::Positive | UnaryOperator::Negative), e) => {
            let value = number(&search_argument(e, params, consumed)?)?;
            Ok(Value::Number(if *op == UnaryOperator::Negative {
                -value
            } else {
                value
            }))
        }
        Expr::FunctionCall {
            name,
            args,
            distinctness,
            order_by,
            within_group,
            filter_over,
        } if matches!(name.as_str(), "__fastdb_h_geo_point" | "vector32")
            && distinctness.is_none()
            && order_by.is_empty()
            && within_group.is_empty()
            && filter_over.filter_clause.is_none()
            && filter_over.over_clause.is_none() =>
        {
            let args = args
                .iter()
                .map(|arg| search_argument(arg, params, consumed))
                .collect::<Result<Vec<_>>>()?;
            if name.as_str() == "vector32" {
                let [Value::String(input)] = args.as_slice() else {
                    return Err(invalid(
                        "vector32 search constructor requires a JSON array string",
                    ));
                };
                let values: Vec<f32> = serde_json::from_str(input)
                    .map_err(|_| invalid("invalid vector32 JSON array"))?;
                Value::vector32(&values)
            } else {
                call("point", &args)
            }
        }
        _ => Err(invalid(
            "search arguments must be literals, parameters, geo::point or vector32 constructors",
        )),
    }
}

impl crate::Connection {
    pub(crate) fn spatial_search_sql(
        &self,
        index: &Value,
        center: &Value,
        radius: &Value,
    ) -> Result<String> {
        let Value::String(name) = index else {
            return Err(invalid("search index name must be a string"));
        };
        let name = crate::canonical(name)?;
        let center = point(center)?;
        let radius = number(radius)?;
        if radius < 0.0 {
            return Err(invalid("radius must be nonnegative"));
        }
        let index = self
            .collections()?
            .into_iter()
            .flat_map(|c| c.indexes)
            .find(|i| i.name == name)
            .ok_or_else(|| crate::Error::NotFound(format!("spatial index {name}")))?;
        if index.kind != crate::IndexKind::Spatial {
            return Err(invalid("search::near requires a spatial index"));
        }
        // Latitude is 1-Lipschitz in central angle. Widen outward before the
        // index range comparison to cover floating point boundary rounding.
        // Refinement always applies the same distance predicate as geo::within.
        let (low, high) = latitude_bounds(center.1, radius);
        let distance = format!(
            "__fastdb_geo_distance(longitude, key, {}, {})",
            center.0, center.1
        );
        Ok(format!("SELECT id, {distance} AS distance_m FROM {} INDEXED BY {} WHERE key >= {low} AND key <= {high} AND {distance} <= {radius}", crate::quote(&index.storage), crate::quote(&index.name)))
    }
}

pub(crate) fn distance_coordinates(args: &[Value]) -> Result<Value> {
    let [ax, ay, bx, by] = args else {
        return Err(invalid("distance coordinate arity"));
    };
    Ok(Value::Number(distance(
        coordinates(ax, ay)?,
        coordinates(bx, by)?,
    )))
}

fn latitude_bounds(latitude: f64, radius: f64) -> (f64, f64) {
    // The inverse haversine is ill-conditioned near antipodes. Widen in
    // haversine space, not just degrees, to cover roundoff in the predicate.
    let angle = (radius / EARTH_RADIUS_M).min(std::f64::consts::PI);
    let h = ((angle / 2.0).sin().powi(2) + 64.0 * f64::EPSILON).min(1.0);
    let delta = 2.0 * h.sqrt().asin().to_degrees();
    (
        (latitude - delta - 1e-9).max(-90.0),
        (latitude + delta + 1e-9).min(90.0),
    )
}

#[cfg(test)]
mod index_tests {
    use crate::{Database, Parameters, Value};
    fn setup() -> (Database, crate::Connection) {
        let db = Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        for sql in [
            "INSERT INTO places {id:places:a,location:geo::point(1,2)}",
            "CREATE SEARCH INDEX locations ON places(location) USING SPATIAL",
        ] {
            c.execute(sql, &Parameters::new()).unwrap();
        }
        (db, c)
    }
    #[test]
    fn spatial_metadata_is_versioned_and_detects_incompatible_schema() {
        let (db, c) = setup();
        let collection = c.catalog("places").unwrap();
        assert_eq!(collection.version, 3);
        let mut metadata = serde_json::to_value(&collection).unwrap();
        metadata["version"] = serde_json::json!(2);
        assert!(crate::catalog::decode(&metadata.to_string(), "places").is_err());
        metadata["version"] = serde_json::json!(3);
        metadata["indexes"][0]["unique"] = serde_json::json!(true);
        assert!(crate::catalog::decode(&metadata.to_string(), "places").is_err());
        c.run("DROP INDEX locations", &[]).unwrap();
        assert!(db.connect().is_err());
    }
    #[test]
    fn spatial_audit_detects_stale_longitude_and_missing_entries() {
        for change in ["UPDATE {storage} SET longitude=3", "DELETE FROM {storage}"] {
            let (_db, c) = setup();
            let index = c.catalog("places").unwrap().indexes.remove(0);
            c.run(
                &change.replace("{storage}", &crate::quote(&index.storage)),
                &[],
            )
            .unwrap();
            assert!(c
                .check_collection_integrity("places", Default::default())
                .is_err());
        }
    }
    #[test]
    fn ordinary_indexes_keep_v1_catalog_encoding_and_reject_spatial_lookup() {
        let (_db, c) = setup();
        c.execute("INSERT INTO docs {n:1}", &Parameters::new())
            .unwrap();
        c.create_index("docs", "n", vec!["n".into()], false)
            .unwrap();
        assert_eq!(c.catalog("docs").unwrap().version, 2);
        let metadata = serde_json::to_value(c.catalog("docs").unwrap()).unwrap();
        assert!(metadata["indexes"][0].get("kind").is_none());
        assert!(c
            .lookup_index("places", "locations", &Value::Integer(1))
            .is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latitude_bounds_cover_distance_roundoff_near_antipodal_poles() {
        let a = (0.0, 89.99999479428419);
        let b = (0.0, -89.9999964108325);
        let radius = distance(a, b);
        let (low, high) = latitude_bounds(a.1, radius);
        assert!(b.1 >= low && b.1 <= high);
    }

    #[test]
    fn nearby_longitudes_do_not_disappear_during_wrapping() {
        let a = (0.0, 0.0);
        let b = (1e-14, 0.0);
        assert!(distance(a, b) > 0.0);
        assert_eq!(distance(a, b), distance(b, a));
    }

    #[test]
    fn rejects_malformed_points_and_nonfinite_values() {
        let origin = call("point", &[Value::Integer(0), Value::Integer(0)]).unwrap();
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(call("point", &[Value::Number(bad), Value::Integer(0)]).is_err());
            assert!(call(
                "within",
                &[origin.clone(), origin.clone(), Value::Number(bad)]
            )
            .is_err());
        }
        let Value::Object(fields) = &origin else {
            unreachable!()
        };
        let mut altitude = fields.clone();
        altitude.insert(
            "coordinates".into(),
            Value::Array(vec![Value::Integer(0); 3]),
        );
        let mut extra = fields.clone();
        extra.insert("extra".into(), Value::Null);
        let mut wrong_type = fields.clone();
        wrong_type.insert("type".into(), Value::String("point".into()));
        let mut out_of_range = fields.clone();
        out_of_range.insert(
            "coordinates".into(),
            Value::Array(vec![Value::Integer(181), Value::Integer(0)]),
        );
        for bad in [altitude, extra, wrong_type, out_of_range] {
            assert!(call("distance", &[origin.clone(), Value::Object(bad)]).is_err());
        }
    }
}
