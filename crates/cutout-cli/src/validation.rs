use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ValidationRow {
    device: &'static str,
    firmware: &'static str,
    variant_scope: &'static str,
    capture_id: &'static str,
    bms_status: &'static str,
    tested_fields: &'static str,
    inferred_fields: &'static str,
    unverified_fields: &'static str,
    controls: &'static str,
    minimum_evidence: &'static str,
    acceptance: &'static str,
}

const VALIDATION_ROWS: &[ValidationRow] = &[
    ValidationRow {
        device: "NOSFET Aero",
        firmware: "v3.8.12",
        variant_scope: "NF2557 / 30s Samsung 50S target",
        capture_id: "aero-nf2557-2026-06-21-powered-on-long",
        bms_status: "30s/2p Veteran smart-BMS; cell pages typed; temperature/current/status gated",
        tested_fields: "identity, telemetry, battery, GATT",
        inferred_fields: "none",
        unverified_fields: "controls, firmware variants",
        controls: "read-only scan, connect, capture",
        minimum_evidence: "additional charging/lift/rolling/BMS-screen captures before full BMS close",
        acceptance: "hardware-tested",
    },
    ValidationRow {
        device: "Begode Falcon",
        firmware: "unknown",
        variant_scope: "84V target hardware; registry selection gated on app/BMS voltage evidence",
        capture_id: "pending capture",
        bms_status: "source-backed 0x01/0x02/0x03 decoder; concrete Falcon layout unverified",
        tested_fields: "identity probe, firmware probe",
        inferred_fields: "telemetry family",
        unverified_fields: "battery, controls, frame mapping",
        controls: "read-only probe only",
        minimum_evidence: "capture advertisement, GATT, PEVCAP replay, and protocol frames",
        acceptance: "inferred",
    },
    ValidationRow {
        device: "Refloat",
        firmware: "unknown",
        variant_scope: "unverified VESC/Refloat target",
        capture_id: "pending capture",
        bms_status: "unknown",
        tested_fields: "none",
        inferred_fields: "family classification",
        unverified_fields: "capture IDs, controls, telemetry",
        controls: "not yet surfaced",
        minimum_evidence: "capture advertisement, GATT, PEVCAP replay, and protocol frames",
        acceptance: "unverified",
    },
];

pub(crate) fn render_validation_report() -> String {
    let mut out = String::new();
    out.push_str("Hardware validation matrix\n");
    out.push_str(
        "Generated from registry entries and capture notes already checked into the tree.\n",
    );
    out.push('\n');
    out.push_str(
        "device | firmware | variant scope | capture id | BMS status | tested fields | inferred fields | unverified fields | controls | minimum evidence | acceptance status\n",
    );
    out.push_str(
        "------ | -------- | ------------- | ---------- | ---------- | ------------- | --------------- | ----------------- | -------- | ---------------- | ----------------\n",
    );

    for row in VALIDATION_ROWS {
        let _ = writeln!(
            out,
            "{} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {}",
            row.device,
            row.firmware,
            row.variant_scope,
            row.capture_id,
            row.bms_status,
            row.tested_fields,
            row.inferred_fields,
            row.unverified_fields,
            row.controls,
            row.minimum_evidence,
            row.acceptance,
        );
    }

    out
}

#[cfg(test)]
mod tests {
    use super::render_validation_report;

    #[test]
    fn validation_report_lists_the_named_families() {
        let report = render_validation_report();

        assert!(report.contains("NOSFET Aero"));
        assert!(report.contains("Begode Falcon"));
        assert!(report.contains("Refloat"));
    }

    #[test]
    fn validation_report_marks_each_acceptance_state() {
        let report = render_validation_report();

        assert!(report.contains("hardware-tested"));
        assert!(report.contains("inferred"));
        assert!(report.contains("unverified"));
    }

    #[test]
    fn validation_report_includes_capture_ids_and_field_categories() {
        let report = render_validation_report();

        assert!(report.contains("aero-nf2557-2026-06-21-powered-on-long"));
        assert!(report.contains("tested fields"));
        assert!(report.contains("inferred fields"));
        assert!(report.contains("unverified fields"));
        assert!(report.contains("controls"));
    }

    #[test]
    fn validation_report_tracks_variants_bms_and_minimum_evidence() {
        let report = render_validation_report();

        assert!(report.contains("variant scope"));
        assert!(report.contains("BMS status"));
        assert!(report.contains("minimum evidence"));
        assert!(report.contains("30s/2p Veteran smart-BMS"));
        assert!(report.contains("84V target hardware; registry selection gated"));
        assert!(report.contains("capture advertisement, GATT, PEVCAP replay, and protocol frames"));
    }
}
