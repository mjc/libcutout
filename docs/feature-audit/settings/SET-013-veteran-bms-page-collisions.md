# SET-013: Veteran/NOSFET cell pages overwrite one another

- Severity: P1
- Evidence status: source-confirmed defect
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

Receive Veteran/NOSFET BMS cell selectors 1, 2, 5 and 6, each with 15 cell values. Their mobile groups all use indices 1–15, so merging replaces earlier page cells with later ones.

## Expected behavior

The cell map must preserve each observed cell with stable page/bank identity. Until pack mapping is established, show separate labeled banks instead of collapsing their values.

## Evidence

crates/cutout-protocols/src/veteran_bms.rs:14-15,64-72,125-172 establishes 15-value cell pages at four selectors. crates/cutout-mobile-ffi/src/lib.rs:11631-11651 projects indices through BmsPageIdentity; 11665-11692 only applies offsets for Begode and otherwise starts at FIRST. CutoutSessionCore.swift:1430-1439 aggregates cached pages; CutoutMobile.swift:5309-5315 overwrites by index.

Production ingress: crates/cutout-protocols/src/session.rs:1428-1453 forwards each Veteran cell page with its selector, without adding a global cell index. This path is declared by crates/cutout-protocols/src/lib.rs:79; the similarly named veteran_session.rs is not the production module.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Introduce verified Veteran page/bank identity in the existing projection; do not invent a 60-series topology from four 15-cell pages.

## Acceptance and reproduction checks

Replay all four distinct pages with unique sentinel voltages. Every observed cell must remain inspectable with unique identity; receiving another page must not relabel earlier cells.

## Limits

Physical pack arrangement is unresolved here. The source proves collision, not a particular series/parallel topology.

