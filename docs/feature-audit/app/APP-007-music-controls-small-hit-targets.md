# APP-007: Music transport button minimum sizes may produce cramped touch targets

- Severity: P3
- Evidence status: hypothesis needing layout validation
- Status: OPEN
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`

## Trigger and actual behavior

Use compact-player previous/play/next/open/close buttons on a phone, especially with limited dexterity. The explicit minimum frames are 30–36 points wide and 36 points tall, separated by four points; actual hit areas depend on intrinsic layout and must be measured.

## Expected behavior

Common playback actions should have comfortably sized, separated hit regions while preserving room for title and settings.

## Evidence

`swift/CutoutMobile/Sources/CutoutMobile/MusicIntegration.swift:1055-1091` groups transport buttons with four-point spacing. `:1119-1133` applies 30/36-by-36 minimum frames and a plain button style; no larger content shape is supplied in that button.

## Minimal remedy direction

Expand tappable areas using native layout while retaining a compact visual glyph. Verify actual layout/hit testing before choosing dimensions.

## Acceptance checks

Inspect actual hit regions on small/large phones and accessibility text sizes. Confirm adjacent controls cannot easily receive the same intended tap and the row remains usable.

## Comparison and limits

This is source-based sizing evidence, not a measured on-phone mis-tap rate. No competitor-specific target size was asserted.
