use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ValidationRow {
    device: &'static str,
    firmware: &'static str,
    capture_id: &'static str,
    tested_fields: &'static str,
    inferred_fields: &'static str,
    unverified_fields: &'static str,
    controls: &'static str,
    acceptance: &'static str,
}

const VALIDATION_ROWS: &[ValidationRow] = &[
    ValidationRow {
        device: "NOSFET Aero",
        firmware: "v3.8.12",
        capture_id: "aero-nf2557-2026-06-21-powered-on-long",
        tested_fields: "identity, telemetry, battery, GATT",
        inferred_fields: "none",
        unverified_fields: "controls, firmware variants",
        controls: "read-only scan, connect, capture",
        acceptance: "hardware-tested",
    },
    ValidationRow {
        device: "Begode Falcon",
        firmware: "unknown",
        capture_id: "pending capture",
        tested_fields: "identity probe, firmware probe",
        inferred_fields: "telemetry family",
        unverified_fields: "battery, controls, frame mapping",
        controls: "read-only probe only",
        acceptance: "inferred",
    },
    ValidationRow {
        device: "Refloat",
        firmware: "unknown",
        capture_id: "pending capture",
        tested_fields: "none",
        inferred_fields: "family classification",
        unverified_fields: "capture IDs, controls, telemetry",
        controls: "not yet surfaced",
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
        "device | firmware | capture id | tested fields | inferred fields | unverified fields | controls | acceptance status\n",
    );
    out.push_str(
        "------ | -------- | ---------- | ------------- | --------------- | ----------------- | -------- | ----------------\n",
    );

    for row in VALIDATION_ROWS {
        let _ = writeln!(
            out,
            "{} | {} | {} | {} | {} | {} | {} | {}",
            row.device,
            row.firmware,
            row.capture_id,
            row.tested_fields,
            row.inferred_fields,
            row.unverified_fields,
            row.controls,
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
}
