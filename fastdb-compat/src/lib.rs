//! Validation and deterministic rendering for FastDB's locked compatibility inventory.

#![forbid(unsafe_code)]
#![deny(warnings)]

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    pub inventory: InventoryMetadata,
    pub capability: Vec<Capability>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryMetadata {
    pub schema_version: u32,
    pub reference: String,
    pub reference_version: String,
    pub active_phase: u8,
    pub granularity_locked: bool,
    pub inventory_date: String,
    pub binary_url: String,
    pub binary_version: String,
    pub binary_sha256: String,
    pub official_docs: Vec<String>,
    pub area_order: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    pub id: String,
    pub area: String,
    pub title: String,
    pub phase: u8,
    pub disposition: Disposition,
    pub status: Status,
    pub syntax: String,
    pub provenance: Vec<String>,
    pub parser_evidence: Vec<String>,
    pub execution_evidence: Vec<String>,
    pub stop_report: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Status {
    Supported,
    Partial,
    Unsupported,
}

impl Status {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Supported => "Supported",
            Self::Partial => "Partial",
            Self::Unsupported => "Unsupported",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Disposition {
    Target,
    Excluded,
    Dormant,
}

impl Inventory {
    pub fn parse(source: &str) -> Result<Self, String> {
        let inventory: Self = toml::from_str(source)
            .map_err(|error| format!("invalid compatibility inventory TOML: {error}"))?;
        inventory.validate()?;
        Ok(inventory)
    }

    pub fn from_path(path: &Path) -> Result<Self, String> {
        let source = std::fs::read_to_string(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        Self::parse(&source)
    }

    pub fn validate(&self) -> Result<(), String> {
        let metadata = &self.inventory;
        if metadata.schema_version != 1 {
            return Err("inventory schema_version must be 1".into());
        }
        if metadata.reference != "SurrealDB v3.1.5" || metadata.reference_version != "3.1.5" {
            return Err("inventory behavioral reference must be SurrealDB v3.1.5".into());
        }
        if !(12..=20).contains(&metadata.active_phase) {
            return Err("inventory active_phase must be in 12..=20".into());
        }
        if !metadata.granularity_locked {
            return Err("inventory granularity must be locked".into());
        }
        if metadata.inventory_date.is_empty()
            || metadata.binary_url.is_empty()
            || metadata.binary_version != "3.1.5 for linux on x86_64"
            || metadata.binary_sha256.len() != 64
            || metadata.official_docs.is_empty()
        {
            return Err("inventory reference provenance is incomplete".into());
        }

        let areas = metadata.area_order.iter().collect::<BTreeSet<_>>();
        if areas.len() != metadata.area_order.len() || areas.iter().any(|area| area.is_empty()) {
            return Err("area_order contains an empty or duplicate area".into());
        }

        let area_positions = metadata
            .area_order
            .iter()
            .enumerate()
            .map(|(position, area)| (area.as_str(), position))
            .collect::<BTreeMap<_, _>>();
        let mut ids = BTreeSet::new();
        let mut previous_area_position = None;
        for capability in &self.capability {
            validate_id(&capability.id)?;
            if !ids.insert(capability.id.as_str()) {
                return Err(format!("duplicate capability ID {}", capability.id));
            }
            let area_position = *area_positions
                .get(capability.area.as_str())
                .ok_or_else(|| format!("unknown area in {}: {}", capability.id, capability.area))?;
            if previous_area_position.is_some_and(|previous| previous > area_position) {
                return Err(format!(
                    "capabilities are not ordered by area_order at {}",
                    capability.id
                ));
            }
            previous_area_position = Some(area_position);
            validate_capability(capability, metadata.active_phase)?;
        }
        if self.capability.len() < 200 {
            return Err("atomic inventory unexpectedly contains fewer than 200 rows".into());
        }
        Ok(())
    }

    pub fn locked_ids(&self) -> String {
        let mut output = self
            .capability
            .iter()
            .map(|capability| capability.id.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        output.push('\n');
        output
    }

    pub fn validate_locked_ids(&self, source: &str) -> Result<(), String> {
        if self.locked_ids() == source {
            Ok(())
        } else {
            Err("capability IDs differ from compat/surrealdb-v3.1.5.ids; an explicit inventory amendment is required".into())
        }
    }

    pub fn render_markdown(&self) -> String {
        let counts = self
            .capability
            .iter()
            .fold([0_usize; 3], |mut counts, item| {
                counts[match item.status {
                    Status::Supported => 0,
                    Status::Partial => 1,
                    Status::Unsupported => 2,
                }] += 1;
                counts
            });
        let mut output = String::new();
        writeln!(output, "# FastDB Compatibility Matrix\n").unwrap();
        writeln!(
            output,
            "<!-- Generated by `turso_fastdb_compat`; edit `compat/surrealdb-v3.1.5.toml`, not this table. -->\n"
        )
        .unwrap();
        writeln!(output, "FastDB implements a clean-room SurrealQL-compatible subset pinned to SurrealDB `v3.1.5`. Parser acceptance alone does not mean execution support, and compatibility does not imply sponsorship or certification. See `CLEAN_ROOM.md`.\n").unwrap();
        writeln!(output, "The locked inventory contains {} atomic capabilities: {} Supported, {} Partial, and {} Unsupported. `Partial` is allowed only for the active Phase {}; all future-phase targets remain Unsupported until their implementation phase.\n", self.capability.len(), counts[0], counts[1], counts[2], self.inventory.active_phase).unwrap();
        writeln!(output, "## Status legend\n").unwrap();
        writeln!(output, "- **Supported**: executable behavior has conformance evidence.\n- **Partial**: an atomic Phase {} capability is actively being implemented.\n- **Unsupported**: unavailable, excluded, dormant, or awaiting its assigned phase.\n", self.inventory.active_phase).unwrap();
        writeln!(output, "Native FTS syntax and ATTACH/DETACH are FastDB extensions and do not count as SurrealQL compatibility. Geospatial/geometry, history/changefeeds/time-series retention, Realtime/LIVE/KILL, GraphQL, GQL, multiprocess access, and parallel writers are advertised as unavailable.\n").unwrap();

        let mut current_area = None;
        for capability in &self.capability {
            if current_area != Some(capability.area.as_str()) {
                current_area = Some(capability.area.as_str());
                writeln!(output, "## {}\n", capability.area).unwrap();
                writeln!(output, "| Capability ID | Status | Delivery | Capability | Exact surface | Provenance | Evidence / stop report |").unwrap();
                writeln!(output, "| --- | --- | --- | --- | --- | --- | --- |").unwrap();
            }
            let delivery = match capability.disposition {
                Disposition::Target => format!("Phase {}", capability.phase),
                Disposition::Excluded => "Excluded".into(),
                Disposition::Dormant => "Dormant".into(),
            };
            let provenance = render_list(&capability.provenance);
            let mut evidence = capability
                .parser_evidence
                .iter()
                .chain(&capability.execution_evidence)
                .cloned()
                .collect::<Vec<_>>();
            if !capability.stop_report.is_empty() {
                evidence.push(capability.stop_report.clone());
            }
            writeln!(
                output,
                "| `{}` | {} | {} | {} | {} | {} | {} |",
                escape_markdown(&capability.id),
                capability.status.as_str(),
                delivery,
                escape_markdown(&capability.title),
                escape_markdown(&capability.syntax),
                provenance,
                render_list(&evidence),
            )
            .unwrap();
        }
        output
    }
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.starts_with('-')
        || id.ends_with('-')
        || id
            .bytes()
            .any(|byte| !(byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'-'))
    {
        return Err(format!("invalid capability ID {id:?}"));
    }
    Ok(())
}

fn validate_capability(capability: &Capability, active_phase: u8) -> Result<(), String> {
    if capability.title.trim().is_empty()
        || capability.syntax.trim().is_empty()
        || capability.provenance.is_empty()
        || capability
            .provenance
            .iter()
            .any(|value| value.trim().is_empty())
        || capability
            .parser_evidence
            .iter()
            .chain(&capability.execution_evidence)
            .any(|value| value.trim().is_empty())
    {
        return Err(format!(
            "{} has incomplete descriptive fields",
            capability.id
        ));
    }
    match capability.disposition {
        Disposition::Target if !(12..=20).contains(&capability.phase) => {
            return Err(format!("{} target phase is outside 12..=20", capability.id));
        }
        Disposition::Excluded | Disposition::Dormant if capability.phase != 0 => {
            return Err(format!(
                "{} unavailable disposition must use phase 0",
                capability.id
            ));
        }
        Disposition::Target => {}
        Disposition::Excluded | Disposition::Dormant => {}
    }
    if capability.status == Status::Partial && capability.phase != active_phase {
        return Err(format!(
            "{} is Partial outside the active phase",
            capability.id
        ));
    }
    if capability.status == Status::Supported && capability.execution_evidence.is_empty() {
        return Err(format!(
            "{} is Supported without execution evidence",
            capability.id
        ));
    }
    if capability.status == Status::Unsupported && !capability.execution_evidence.is_empty() {
        return Err(format!(
            "{} is Unsupported but has execution evidence",
            capability.id
        ));
    }
    if matches!(
        capability.disposition,
        Disposition::Excluded | Disposition::Dormant
    ) && capability.status != Status::Unsupported
    {
        return Err(format!(
            "{} unavailable disposition must be Unsupported",
            capability.id
        ));
    }
    if !capability.stop_report.is_empty()
        && (capability.status != Status::Unsupported
            || capability.disposition != Disposition::Target)
    {
        return Err(format!(
            "{} has an invalid architecture stop report",
            capability.id
        ));
    }
    Ok(())
}

fn render_list(values: &[String]) -> String {
    if values.is_empty() {
        "—".into()
    } else {
        values
            .iter()
            .map(|value| escape_markdown(value))
            .collect::<Vec<_>>()
            .join("<br>")
    }
}

fn escape_markdown(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_fields() {
        let error = Inventory::parse("[inventory]\nunknown = true\n")
            .expect_err("unknown metadata must be rejected");
        assert!(error.contains("unknown field"));
    }
}
