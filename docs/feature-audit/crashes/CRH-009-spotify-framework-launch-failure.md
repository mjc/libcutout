# CRH-009: Some installed builds could not launch because Spotify was missing

- Severity: P1
- Evidence status: Phone-confirmed historical launch failure; current artifact regression check open
- Status: OPEN — no current phone acceptance test
- Audited source: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`

## Observed behavior

Three September 6 launch reports terminate in DYLD with Library missing for @rpath/SpotifyiOS.framework/SpotifyiOS. This happened before application feature code could run.

## Expected behavior

Every installed build launches whether or not the rider enables music.

## Evidence and current source

`swift/CutoutMobile/Package.swift:23` declares the SpotifyiOS package product for iOS. Package declaration or successful linking does not by itself demonstrate that the installed app contains the required runtime framework.

Reports (hashes and binary UUIDs in [phone evidence](../phone-evidence.md)):

- `CutoutApp-2026-09-06-154708.ips`
- `CutoutApp-2026-09-06-154709.ips`
- `CutoutApp-2026-09-06-154953.ips`

## Minimal remedy direction

Inspect the signed install artifact’s runtime dependencies, embedded frameworks and runpaths; verify the repository packaging/install boundary rather than adding a local machine search path.

## Acceptance checks

Install the exact signed artifact on the phone and cold-launch with and without Spotify configured; match binary UUID, verify framework embedding/signature and absence of DYLD failures.

## Comparison and limits

Keeping the app responsive is a baseline expectation, independent of competitor design. Today’s later-stage crashes show that some subsequent binaries launched successfully. That does not establish a regression-proof packaging path for every build configuration; this remains open until verified.
