# SET-005: Aero toggle and riding-mode drafts outlive rejected writes

- Severity: P2
- Evidence status: source-confirmed defect
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

Change high-speed, low-battery, transport, riding mode or an additional numeric setting, submit it, then receive a refusal or a differing readback. The draft remains the editor’s permanent preferred value.

## Expected behavior

Keep unsent edits explicit, and after submission clearly distinguish rejected/requested/current state; offer retry or reset to observed state.

## Evidence

swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1018-1039,1062-1080,1108-1135 each prefers `draftValue ?? currentValue` but never clears draftValue after sending or readback. CutoutAppModel.swift:2413-2417 refreshes model state, which cannot replace that local draft. The toggle and riding-mode controls have no adjacent current-value row.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Reconcile draft lifecycle with the existing Rust setting state; show current and pending/requested values beside controls and clear/reset drafts deliberately.

## Acceptance and reproduction checks

Submit a toggle, inject refusal and opposing device readback; the UI must not imply the draft became current. Confirm a successful write, then inject a later external change.

## Limits

This does not claim the device accepts refused values. It is an editor-state defect that can conceal correct Rust readback.

