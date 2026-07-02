# CutOut BMS mockup notes

These screens are disposable wiring references for BMS support across wildly different pack sizes and reporting quality. They intentionally do not assume EUC-only pack sizes.

## Screens

1. `cutout-05-bms-overview` — topology summary, usable energy, pack voltage, cell delta, lowest group, highest temp, balancing, fault state.
2. `cutout-06-bms-cell-map-6s` — small skateboard-style pack. All series groups are visible inline with exact values.
3. `cutout-07-bms-cell-map-40s` — large EUC-style pack. Overview heatmap first, exact cell/group table via horizontal scroll or full-screen detail.
4. `cutout-08-bms-cell-detail-popover` — tap target model for any cell/group. Shows exact voltage, deviation, temp, internal-resistance estimate, trend, and export/compare actions.
5. `cutout-09-bms-unknown-topology` — partial/unknown BMS state. Avoids fake certainty and points toward the later unsupported-device capture flow.
6. `cutout-10-dumb-bms-no-data` / `MockupScreenID.bmsNoData` — controller-only battery estimate for non-smart BMS devices. Distinct from BMS telemetry screens: it shows what CutOut can still infer while keeping cell-level, fault, and temperature visibility explicitly unknown.

## Layout rules

Small packs, roughly 6S–14S: show every series group inline. Do not waste space with a giant abstract heatmap.

Medium packs, roughly 15S–24S: show grouped rows and exact values. Scrolling is acceptable, but anomalies should remain pinned at the top.

Large packs, roughly 25S–40S+: show an overview heatmap or strip first. Exact per-cell/group data should be horizontal-scroll, full-screen, or popover-driven. Do not put 40+ tiny labels on a ride screen.

Split packs: display topology explicitly, such as `20S4P split pack`, `2 × 20S`, `master + slave BMS`, or `left/right pack`. The UI should preserve pack identity instead of flattening everything into one anonymous list.

Unknown topology: show raw-safe values, mark confidence, and avoid naming cells/groups until the mapping is known.

No-data controller-only estimate: treat this as its own layout, not another size tier. Emphasize the estimate/confidence split, keep unknowns explicit, and avoid implying cell topology or BMS fault visibility that the device does not report.

## Data model hints

A BMS screen wants a topology object, not just an array of voltages. Useful fields:

- series_group_count
- parallel_count, if known
- pack_count / module_count
- bms_count
- mapping confidence
- group voltages
- group temperatures, if available
- balancing state per group
- charge/discharge FET state
- current, voltage, SOC, SOH
- fault bits plus decoded fault labels
- sample timestamp and source

The UI should treat `missing`, `unknown`, `stale`, and `known zero` as different states.
