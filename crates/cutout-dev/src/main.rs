use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    ffi::OsStr,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

use anyhow::{Context, Result, bail, ensure};
use cutout_core::{
    AeroAngleAdjustment, AeroBeeperVolume, AeroBrakeOverpressureAlarm, AeroDisplayBacklight,
    AeroDynamicAssist, AeroHighSpeedMode, AeroLateralTiltLimit, AeroLowBatteryMode,
    AeroMaxChargeVoltageRaw, AeroPedalDipCompensation, AeroPedalHardness, AeroPwmPercent,
    AeroPwmSetting, AeroRidingMode, AeroSpeedSetting, AeroTransportMode, AeroVoltageCorrection,
    AeroWheelUnits, DeviceCommand, LightState, MonotonicTimestamp, PedalMode, RideOperatingState,
};
use cutout_protocols::AeroSettingsSimulator;
use serde_json::Value;
use sha2::{Digest, Sha256};

const GENERATED_PACKAGE: &str = "target/swift-ffi";
const CARGO_SWIFT_PACKAGE: &str = "crates/cutout-mobile-ffi/CutoutMobileFFI";
const SWIFT_FFI_LOCK: &str = "target/swift-ffi/.generation.lock";
const FFI_RECEIPT: &str = ".cutout-artifact.json";
const FFI_CHECKER: &str = ".cutout-ffi-check";
const FFI_GENERATIONS: &str = "target/swift-ffi/generations";

fn lock_swift_ffi(root: &Path) -> Result<fs::File> {
    let path = root.join(SWIFT_FFI_LOCK);
    fs::create_dir_all(path.parent().context("FFI lock parent")?)?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    // Kernel ownership lasts until the handle closes, including process death.
    // Never unlink the file: waiters must all lock the same inode.
    lock.lock().context("lock Swift FFI publication")?;
    Ok(lock)
}

#[derive(Debug, Eq, PartialEq)]
enum DevCommand {
    AeroSettingsSimulator,
    SwiftFfi,
    Swift(Vec<String>),
    Xcodebuild(Vec<String>),
    SwiftFfiCheck {
        root: PathBuf,
        package: PathBuf,
        output: PathBuf,
    },
    IosDeploy(Vec<String>),
    IosVerifyApp(PathBuf),
    IosCaptures,
}

fn main() -> Result<()> {
    let root = workspace_root();
    let args = env::args().skip(1).collect::<Vec<_>>();
    match parse_cli(&args)? {
        DevCommand::AeroSettingsSimulator => run_aero_settings_simulator(),
        DevCommand::SwiftFfi => ensure_swift_ffi(&root),
        DevCommand::Swift(args) => build_apple_client(&root, "swift", &args),
        DevCommand::Xcodebuild(args) => build_apple_client(&root, "xcodebuild", &args),
        DevCommand::SwiftFfiCheck {
            root,
            package,
            output,
        } => check_swift_ffi_build(&root, &package, &output),
        DevCommand::IosDeploy(launch_args) => deploy_ios(&root, &launch_args),
        DevCommand::IosVerifyApp(product) => verify_ios_app(&product),
        DevCommand::IosCaptures => pull_ios_captures(&root),
    }
}

fn parse_cli(args: &[String]) -> Result<DevCommand> {
    match args {
        [simulator, scenario] if simulator == "simulator" && scenario == "aero-settings" => {
            Ok(DevCommand::AeroSettingsSimulator)
        }
        [command] if command == "swift-ffi" => Ok(DevCommand::SwiftFfi),
        [command, separator, arguments @ ..] if command == "swift" && separator == "--" => {
            Ok(DevCommand::Swift(arguments.to_vec()))
        }
        [command, separator, arguments @ ..] if command == "xcodebuild" && separator == "--" => {
            Ok(DevCommand::Xcodebuild(arguments.to_vec()))
        }
        [command, root, package, output] if command == "swift-ffi-check" => {
            Ok(DevCommand::SwiftFfiCheck {
                root: root.into(),
                package: package.into(),
                output: output.into(),
            })
        }
        [ios, captures] if ios == "ios" && captures == "captures" => Ok(DevCommand::IosCaptures),
        [ios, verify, product] if ios == "ios" && verify == "verify-app" => {
            Ok(DevCommand::IosVerifyApp(product.into()))
        }
        [ios, deploy] if ios == "ios" && deploy == "deploy" => {
            Ok(DevCommand::IosDeploy(Vec::new()))
        }
        [ios, deploy, separator, launch_args @ ..]
            if ios == "ios" && deploy == "deploy" && separator == "--" =>
        {
            Ok(DevCommand::IosDeploy(launch_args.to_vec()))
        }
        _ => bail!(
            "usage: cutout-dev simulator aero-settings | cutout-dev swift-ffi | cutout-dev swift -- <args> | cutout-dev xcodebuild -- <args> | cutout-dev ios deploy [-- <launch args>...] | cutout-dev ios verify-app <app bundle> | cutout-dev ios captures"
        ),
    }
}

fn build_apple_client(root: &Path, tool: &str, args: &[String]) -> Result<()> {
    ensure!(
        !args
            .iter()
            .any(|arg| matches!(arg.as_str(), "--skip-build" | "test-without-building")),
        "the FFI build pipeline requires a build; skip-build cannot verify the executable"
    );
    let lock = lock_swift_ffi(root)?;
    prepare_swift_ffi(root, &lock)?;
    run(
        command("/usr/bin/xcrun")
            .current_dir(root)
            .arg(tool)
            .args(args),
        "build Apple client",
    )
}

fn run_aero_settings_simulator() -> Result<()> {
    let commands = [
        DeviceCommand::SetAeroTiltbackSpeed(
            AeroSpeedSetting::new(53).context("53 km/h is a valid Aero tiltback speed")?,
        ),
        DeviceCommand::SetAeroPwmPercent(AeroPwmSetting::Margin(
            AeroPwmPercent::new(64).context("64% is a valid Aero PWM setting")?,
        )),
        DeviceCommand::SetAeroAlarmSpeed(
            AeroSpeedSetting::new(56).context("56 km/h is a valid Aero alarm speed")?,
        ),
        DeviceCommand::SetAeroAngleAdjustment(
            AeroAngleAdjustment::new(-12).context("-1.2 degrees is a valid Aero angle")?,
        ),
        DeviceCommand::SetPedalMode(PedalMode::Hard),
        DeviceCommand::SetAeroRidingMode(AeroRidingMode::Medium),
        DeviceCommand::SetAeroPedalHardness(
            AeroPedalHardness::new(64).context("64% is a source-documented MD hardness")?,
        ),
        DeviceCommand::SetAeroDisplayBacklight(
            AeroDisplayBacklight::new(80).context("80% is a valid Aero backlight")?,
        ),
        DeviceCommand::SetAeroBeeperVolume(
            AeroBeeperVolume::new(40).context("40% is a valid Aero beeper volume")?,
        ),
        DeviceCommand::SetAeroDynamicAssist(
            AeroDynamicAssist::new(35).context("35% is a valid Aero dynamic assist")?,
        ),
        DeviceCommand::SetAeroPedalDipCompensation(
            AeroPedalDipCompensation::new(25)
                .context("25% is a valid Aero pedal-dip compensation")?,
        ),
        DeviceCommand::SetAeroLateralTiltLimit(
            AeroLateralTiltLimit::new(55).context("55 degrees is a valid Aero lateral limit")?,
        ),
        DeviceCommand::SetAeroVoltageCorrection(
            AeroVoltageCorrection::new(-5).context("-0.5% is a valid Aero voltage correction")?,
        ),
        DeviceCommand::SetAeroMaxChargeVoltageRaw(
            AeroMaxChargeVoltageRaw::new(46).context("46 is a valid raw Aero MxV value")?,
        ),
        DeviceCommand::SetAeroWheelUnits(AeroWheelUnits::Imperial),
        DeviceCommand::SetAeroHighSpeedMode(AeroHighSpeedMode::new(true)),
        DeviceCommand::SetAeroLowBatteryMode(AeroLowBatteryMode::new(false)),
        DeviceCommand::SetAeroTransportMode(AeroTransportMode::new(true)),
        DeviceCommand::SetAeroBrakeOverpressureAlarm(
            AeroBrakeOverpressureAlarm::new(110)
                .context("110% is a valid Aero brake overpressure alarm")?,
        ),
        DeviceCommand::SetAeroGyroCalibration,
        DeviceCommand::SetAeroHighBeam(LightState::On),
        DeviceCommand::SetLights(LightState::On),
        DeviceCommand::ResetTripMeter,
    ];
    let mut simulator = AeroSettingsSimulator::default();
    println!("model={}", AeroSettingsSimulator::registry_entry().model);
    println!(
        "gatt_fingerprints={:?}",
        AeroSettingsSimulator::gatt_fingerprints()
    );

    for (index, command) in commands.into_iter().enumerate() {
        let high_beam = matches!(command, DeviceCommand::SetAeroHighBeam(_));
        let before = simulator.writes().len();
        let monotonic_ms =
            10 + u64::try_from(index).context("scenario index fits in a timestamp")?;
        let _ = simulator.issue(
            command,
            RideOperatingState::Parked,
            None,
            MonotonicTimestamp::new(monotonic_ms),
        );
        if high_beam {
            let _ = simulator.tick(MonotonicTimestamp::new(monotonic_ms + 1));
        }
        println!("command={command:?}");
        for write in &simulator.writes()[before..] {
            println!(
                "  write channel={:?} mode={:?} payload={}",
                write.channel,
                write.mode,
                hex(write.payload.as_slice())
            );
        }
        println!("  readback={:?}", simulator.readback());
    }
    Ok(())
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("cutout-dev must remain under crates/")
        .to_path_buf()
}

fn ensure_swift_ffi(root: &Path) -> Result<()> {
    let lock = lock_swift_ffi(root)?;
    prepare_swift_ffi(root, &lock)
}

fn prepare_swift_ffi(root: &Path, lock: &fs::File) -> Result<()> {
    let expected = source_fingerprint(root)?;
    // Always ask Cargo. A second source/environment cache cannot reproduce its
    // dependency, profile, build-script and toolchain invalidation rules.
    regenerate_swift_ffi(root, &expected, lock)?;
    Ok(())
}

fn regenerate_swift_ffi(root: &Path, expected: &str, lock: &fs::File) -> Result<()> {
    ensure!(
        cfg!(target_os = "macos"),
        "Swift FFI artifact is stale or missing; regenerate it on macOS with `cargo cutout swift-ffi` before using Swift builds on this host"
    );
    ensure_empty_wrapper("RUSTC_WRAPPER")?;
    ensure_empty_wrapper("RUSTC_WORKSPACE_WRAPPER")?;

    // Compile a private copy, never the editor's mutable working tree.
    let snapshot = FfiSourceSnapshot::capture(root, expected)?;
    let cargo_package = snapshot.path.join(CARGO_SWIFT_PACKAGE);
    if cargo_package.exists() {
        // Only an unpublished generator output, never a consumer's generation.
        fs::remove_dir_all(&cargo_package)?;
    }
    run(
        command("cargo")
            // The child retains the same kernel lock if this coordinator is killed.
            .stdin(lock.try_clone()?)
            .current_dir(snapshot.path.join("crates/cutout-mobile-ffi"))
            .env(
                "CARGO_TARGET_DIR",
                root.join("target/swift-ffi/rust-target"),
            )
            .args([
                "swift",
                "package",
                "--platforms",
                "ios@18",
                "macos@15",
                "--release",
                "--name",
                "CutoutMobileFFI",
                "--lib-type",
                "static",
                "--exclude-arch",
                "x86_64-apple-ios",
                "--exclude-arch",
                "x86_64-apple-darwin",
                "--skip-toolchains-check",
                "--accept-all",
                "--swift-tools-version",
                "6.0",
                "--silent",
            ]),
        "generate Swift FFI package",
    )?;
    ensure!(
        source_fingerprint(&snapshot.path)? == expected,
        "Cargo changed the FFI source snapshot"
    );
    sort_xcframework_plist(&cargo_package)?;
    trim_generated_sources(&cargo_package)?;
    seal_swift_ffi(root, &cargo_package, expected)?;
    publish_swift_ffi(root, &cargo_package, expected)?;
    Ok(())
}

fn seal_swift_ffi(root: &Path, package: &Path, expected: &str) -> Result<()> {
    ensure!(
        source_fingerprint(root)? == expected,
        "Rust inputs changed during Swift FFI generation; refusing to publish a mixed build"
    );
    fs::copy(env::current_exe()?, package.join(FFI_CHECKER))?;
    // A Rust implementation-only change must also change a Swift compilation input.
    fs::write(
        package.join("Sources/CutoutMobileFFI/CutoutArtifactIdentity.swift"),
        format!(
            "// Generated Rust source identity.\npublic let cutoutRustSourceIdentity = \"{expected}\"\n"
        ),
    )?;
    verify_swift_ffi(package)?;
    write_ffi_receipt(package, expected)?;
    Ok(())
}

/// Consumers pin a canonical generation path; publication never mutates it.
fn publish_swift_ffi(root: &Path, staged: &Path, source: &str) -> Result<PathBuf> {
    let identity = verify_ffi_receipt(staged, source)?;
    let generation = root
        .join(FFI_GENERATIONS)
        .join(&identity)
        .join("CutoutMobileFFI");
    if !generation.exists()
        || verify_ffi_receipt(&generation, source).ok().as_deref() != Some(&identity)
    {
        let parent = generation.parent().context("generation parent")?;
        fs::create_dir_all(parent)?;
        let staging = tempfile::Builder::new()
            .prefix(".artifact-")
            .tempdir_in(parent)?;
        let mut files = BTreeSet::new();
        collect_files(staged, Path::new(""), &mut files)?;
        for relative in files {
            let destination = staging.path().join(&relative);
            fs::create_dir_all(destination.parent().context("artifact parent")?)?;
            fs::copy(staged.join(relative), destination)?;
        }
        verify_ffi_receipt(staging.path(), source)?;
        if generation.exists() {
            // Preserve the damaged generation for diagnosis; never edit a pinned tree.
            quarantine(&generation)?;
        }
        fs::rename(staging.path(), &generation)?;
    }
    let current = root.join(GENERATED_PACKAGE).join("Package.swift");
    let next = current.with_file_name(format!(".Package.swift-{}", std::process::id()));
    let manifest = ffi_selector_manifest(&identity);
    if fs::read_to_string(&current).ok().as_deref() != Some(&manifest) {
        fs::write(&next, manifest)?;
        fs::rename(next, current)?;
    }
    fs::remove_dir_all(staged)?;
    Ok(generation)
}

fn ffi_selector_manifest(generation: &str) -> String {
    format!(
        r#"// swift-tools-version: 6.0
// cutout-generation: {generation}
import PackageDescription
let package = Package(
    name: "CutoutMobileFFI",
    platforms: [.iOS(.v18), .macOS(.v15)],
    products: [.library(name: "CutoutMobileFFI", targets: ["CutoutMobileFFI"])],
    targets: [
        .binaryTarget(
            name: "cutout_mobile_ffiFFI",
            path: "generations/{generation}/CutoutMobileFFI/cutout_mobile_ffiFFI.xcframework"
        ),
        .target(
            name: "CutoutMobileFFI",
            dependencies: ["cutout_mobile_ffiFFI"],
            path: "generations/{generation}/CutoutMobileFFI/Sources/CutoutMobileFFI"
        ),
    ]
)
"#
    )
}

fn selected_ffi_generation(root: &Path) -> Result<PathBuf> {
    let manifest = fs::read_to_string(root.join(GENERATED_PACKAGE).join("Package.swift"))?;
    let generation = manifest
        .lines()
        .find_map(|line| line.strip_prefix("// cutout-generation: "))
        .context("missing Swift FFI generation selection")?;
    ensure!(
        generation.len() == 64 && generation.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid Swift FFI generation selection"
    );
    let package = root
        .join(FFI_GENERATIONS)
        .join(generation)
        .join("CutoutMobileFFI");
    ensure!(
        manifest == ffi_selector_manifest(generation),
        "Swift FFI selector manifest changed after publication"
    );
    Ok(package)
}

fn deploy_ios(root: &Path, launch_args: &[String]) -> Result<()> {
    ensure!(
        cfg!(target_os = "macos"),
        "iPhone deployment requires macOS"
    );
    let lock = lock_swift_ffi(root)?;
    prepare_swift_ffi(root, &lock)?;

    let device = match env::var("CUTOUT_IOS_DEVICE_UDID") {
        Ok(device) => device,
        Err(_) => discover_ios_device(root)?,
    };
    let derived_data = root.join("target/xcode-device-signed");
    let product = derived_data.join("Build/Products/Debug-iphoneos/CutoutApp.app");
    let team = env::var("CUTOUT_IOS_DEVELOPMENT_TEAM")
        .context("CUTOUT_IOS_DEVELOPMENT_TEAM is required for iPhone deployment")?;
    let spotify_client_id = spotify_client_id()?;
    let bundle_id = env::var("CUTOUT_IOS_APP_BUNDLE_ID").ok();
    let destination = format!("platform=iOS,id={device}");
    let mut build = command("xcodebuild");
    build.current_dir(root.join("swift/CutoutMobile")).args([
        "-project",
        "CutoutApp.xcodeproj",
        "-scheme",
        "CutoutApp",
        "-destination",
        &destination,
        "-derivedDataPath",
    ]);
    build.arg(&derived_data).arg("-allowProvisioningUpdates");
    build.args(ios_signing_arguments(Some(&team), bundle_id.as_deref())?);
    build.arg(format!("SPOTIFY_CLIENT_ID={spotify_client_id}"));
    build.arg("build");
    run(&mut build, "build signed iPhone app")?;
    drop(lock);
    ensure!(
        product.is_dir(),
        "Xcode did not produce {}",
        product.display()
    );

    verify_ios_app(&product)?;
    let bundle_id = plist_value(&product.join("Info.plist"), ":CFBundleIdentifier")?;
    let embedded_spotify_client_id = plist_value(&product.join("Info.plist"), ":SpotifyClientID")?;
    ensure!(
        embedded_spotify_client_id == spotify_client_id,
        "built app does not contain the configured Spotify client ID"
    );
    run(
        command("xcrun").args([
            OsStr::new("devicectl"),
            OsStr::new("--quiet"),
            OsStr::new("device"),
            OsStr::new("install"),
            OsStr::new("app"),
            OsStr::new("--device"),
            device.as_ref(),
            product.as_os_str(),
        ]),
        "install iPhone app",
    )?;
    let mut launch = command("xcrun");
    launch
        .args([
            "devicectl",
            "--quiet",
            "device",
            "process",
            "launch",
            "--device",
            &device,
            "--terminate-existing",
            "--activate",
            &bundle_id,
        ])
        .args(launch_args);
    run(&mut launch, "launch iPhone app")?;

    println!("ios_device_udid={device}");
    println!("ios_app_bundle_id={bundle_id}");
    println!("ios_app_product={}", product.display());
    Ok(())
}

fn pull_ios_captures(root: &Path) -> Result<()> {
    ensure!(cfg!(target_os = "macos"), "iOS capture pull requires macOS");
    let device =
        env::var("CUTOUT_IOS_DEVICE_UDID").map_or_else(|_| discover_ios_device(root), Ok)?;
    let bundle =
        env::var("CUTOUT_IOS_APP_BUNDLE_ID").unwrap_or_else(|_| "io.cutout.cutoutapp".into());
    let destination = env::var_os("CUTOUT_IOS_CAPTURE_DESTINATION")
        .map_or_else(|| root.join("target/ios-captures"), PathBuf::from);
    let limit = env::var("CUTOUT_IOS_CAPTURE_LIMIT")
        .unwrap_or_else(|_| "5".into())
        .parse::<usize>()
        .context("CUTOUT_IOS_CAPTURE_LIMIT must be a positive integer")?;
    ensure!(limit > 0, "CUTOUT_IOS_CAPTURE_LIMIT must be positive");
    fs::create_dir_all(root.join("target"))?;
    let listing = root.join(format!(
        "target/devicectl-captures-{}.json",
        std::process::id()
    ));
    let listed = run(
        command("xcrun")
            .args([
                "devicectl",
                "--quiet",
                "device",
                "info",
                "files",
                "--device",
                &device,
                "--domain-type",
                "appDataContainer",
                "--domain-identifier",
                &bundle,
                "--subdirectory",
                "Documents",
                "--json-output",
            ])
            .arg(&listing),
        &format!("list captures on device {device} in {bundle}"),
    );
    let bytes = fs::read(&listing);
    let _ = fs::remove_file(&listing);
    listed?;
    let document = serde_json::from_slice(&bytes?)?;
    let captures = capture_names(&document, limit)?;
    ensure!(
        !captures.is_empty(),
        "no cutout-btle-capture JSONL files found in {bundle} Documents"
    );
    fs::create_dir_all(&destination)?;
    for name in captures {
        let path = destination.join(name);
        run(
            command("xcrun")
                .args([
                    "devicectl",
                    "--quiet",
                    "device",
                    "copy",
                    "from",
                    "--device",
                    &device,
                    "--domain-type",
                    "appDataContainer",
                    "--domain-identifier",
                    &bundle,
                    "--source",
                    &format!("Documents/{name}"),
                    "--destination",
                ])
                .arg(&path),
            &format!("copy {name} from device {device} in {bundle}"),
        )?;
        println!("{}", path.display());
    }
    Ok(())
}

fn capture_names(document: &Value, limit: usize) -> Result<Vec<&str>> {
    let files = document["result"]["files"]
        .as_array()
        .context("device listing has no files array")?;
    let mut captures = files
        .iter()
        .filter_map(|file| {
            let name = file["name"].as_str()?;
            (name.starts_with("cutout-btle-capture-")
                && Path::new(name).extension() == Some(OsStr::new("jsonl"))
                && !name.contains(['/', '\\']))
            .then(|| {
                (
                    name,
                    file["metadata"]["lastModDate"].as_str().unwrap_or_default(),
                )
            })
        })
        .collect::<Vec<_>>();
    captures.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    Ok(captures
        .into_iter()
        .take(limit)
        .map(|(name, _)| name)
        .collect())
}

fn verify_ios_app(product: &Path) -> Result<()> {
    let output = command("/usr/bin/plutil")
        .args(["-convert", "json", "-o", "-", "--"])
        .arg(product.join("Info.plist"))
        .output()?;
    ensure_success(output.status, "read built app metadata")?;
    verify_ios_metadata(&serde_json::from_slice(&output.stdout)?)
}

fn verify_ios_metadata(metadata: &Value) -> Result<()> {
    let expected = serde_json::json!({
        "CFBundleDisplayName": "CutOut",
        "NSBluetoothAlwaysUsageDescription": "CutOut uses Bluetooth to read live vehicle telemetry.",
        "UIDeviceFamily": [1],
        "UISupportedInterfaceOrientations": [
            "UIInterfaceOrientationPortrait",
            "UIInterfaceOrientationLandscapeLeft",
            "UIInterfaceOrientationLandscapeRight"
        ]
    });
    for (key, value) in expected
        .as_object()
        .expect("metadata expectations are an object")
    {
        ensure!(
            metadata[key] == *value,
            "app metadata mismatch for {key}: expected {value}, got {}",
            metadata[key]
        );
    }
    Ok(())
}

fn spotify_client_id() -> Result<String> {
    for name in ["CUTOUT_SPOTIFY_CLIENT_ID", "SPOTIFY_CLIENT_ID"] {
        if let Ok(value) = env::var(name)
            && !value.trim().is_empty()
        {
            return Ok(value);
        }
    }
    let path = env::var_os("CUTOUT_SPOTIFY_CLIENT_ID_FILE")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .map(|path| path.join("libcutout/spotify-client-id"))
        })
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .map(|path| path.join(".config/libcutout/spotify-client-id"))
        })
        .context("CUTOUT_SPOTIFY_CLIENT_ID is required for iPhone deployment")?;
    let value = fs::read_to_string(&path)
        .with_context(|| format!("read Spotify client ID from {}", path.display()))?;
    let value = value.trim().to_owned();
    ensure!(!value.is_empty(), "Spotify client ID must not be empty");
    Ok(value)
}

fn discover_ios_device(root: &Path) -> Result<String> {
    let output = device_list_output_path(root, std::process::id());
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    run(
        command("xcrun")
            .args(["devicectl", "--quiet", "list", "devices", "--json-output"])
            .arg(&output),
        "list connected iOS devices",
    )?;
    let bytes = fs::read(&output);
    let cleanup = fs::remove_file(&output);
    cleanup?;
    let document: Value = serde_json::from_slice(&bytes?)?;
    document["result"]["devices"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|device| {
            device["hardwareProperties"]["platform"] == "iOS"
                && device["hardwareProperties"]["reality"] == "physical"
                && device["properties"]["state"]["bootState"] == "booted"
        })
        .and_then(|device| device["properties"]["hardware"]["udid"].as_str())
        .map(str::to_owned)
        .context("no connected booted physical iOS device found")
}

fn ios_signing_arguments(team: Option<&str>, bundle_id: Option<&str>) -> Result<Vec<String>> {
    let team = team.context("CUTOUT_IOS_DEVELOPMENT_TEAM is required for iPhone deployment")?;
    let mut arguments = vec![
        "CODE_SIGNING_ALLOWED=YES".to_owned(),
        "CODE_SIGNING_REQUIRED=YES".to_owned(),
        "CODE_SIGN_STYLE=Automatic".to_owned(),
        "CODE_SIGN_IDENTITY=Apple Development".to_owned(),
        format!("DEVELOPMENT_TEAM={team}"),
    ];
    if let Some(bundle_id) = bundle_id {
        arguments.push(format!("PRODUCT_BUNDLE_IDENTIFIER={bundle_id}"));
    }
    Ok(arguments)
}

fn device_list_output_path(root: &Path, process_id: u32) -> PathBuf {
    root.join(format!("target/devicectl-devices-{process_id}.json"))
}

fn source_fingerprint(root: &Path) -> Result<String> {
    Ok(fingerprint_source_inputs(&source_inputs(root)?))
}

fn source_inputs(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    let mut files = BTreeSet::from([
        PathBuf::from("Cargo.lock"),
        PathBuf::from("Cargo.toml"),
        PathBuf::from("rust-toolchain.toml"),
    ]);
    for input in [
        ".cargo/config",
        ".cargo/config.toml",
        "devenv.nix",
        "devenv.yaml",
        "devenv.lock",
        "README.md",
    ] {
        if root.join(input).is_file() {
            files.insert(PathBuf::from(input));
        }
    }
    // Include workspace sources, including transitive FFI dependencies and the
    // generator itself. Generated packages and build artifacts are not inputs.
    for entry in fs::read_dir(root.join("crates"))? {
        let path = entry?.path();
        if !path.join("Cargo.toml").is_file() {
            continue;
        }
        let relative = path.strip_prefix(root)?;
        for directory in [
            "src", "registry", "tests", "benches", "fixtures", "examples",
        ] {
            collect_files(root, &relative.join(directory), &mut files)?;
        }
        for name in ["Cargo.toml", "build.rs", "uniffi.toml"] {
            if path.join(name).is_file() {
                files.insert(relative.join(name));
            }
        }
    }

    files
        .into_iter()
        .map(|relative| {
            let bytes = fs::read(root.join(&relative))
                .with_context(|| format!("reading fingerprint input {}", relative.display()))?;
            Ok((relative, bytes))
        })
        .collect()
}

fn fingerprint_source_inputs(inputs: &BTreeMap<PathBuf, Vec<u8>>) -> String {
    let mut aggregate = Sha256::new();
    for (relative, bytes) in inputs {
        let file_hash = hex(Sha256::digest(bytes));
        aggregate.update(relative.as_os_str().as_encoded_bytes());
        aggregate.update(b"  ");
        aggregate.update(file_hash.as_bytes());
        aggregate.update(b"  ");
        aggregate.update(relative.as_os_str().as_encoded_bytes());
        aggregate.update(b"\n");
    }
    hex(aggregate.finalize())
}

struct FfiSourceSnapshot {
    path: PathBuf,
}

impl FfiSourceSnapshot {
    fn capture(root: &Path, expected: &str) -> Result<Self> {
        let inputs = source_inputs(root)?;
        ensure!(
            fingerprint_source_inputs(&inputs) == expected,
            "Rust inputs changed before snapshot capture"
        );
        // A sibling retains Cargo's ancestor/global config lookup without
        // loading the project's copied .cargo/config.toml a second time.
        let canonical = fs::canonicalize(root)?;
        let repository = hex(Sha256::digest(canonical.as_os_str().as_encoded_bytes()));
        let parent = canonical
            .parent()
            .context("repository parent")?
            .join(".cutout-ffi-sources")
            .join(repository);
        let path = parent.join(expected);
        if path.exists() && source_fingerprint(&path).is_ok_and(|actual| actual == expected) {
            return Ok(Self { path });
        }
        fs::create_dir_all(&parent)?;
        let staging = tempfile::Builder::new()
            .prefix(".source-")
            .tempdir_in(parent)?;
        for (relative, bytes) in inputs {
            let destination = staging.path().join(relative);
            fs::create_dir_all(destination.parent().context("source parent")?)?;
            fs::write(destination, bytes)?;
        }
        if path.exists() {
            // Cargo can leave a modified lockfile after an interrupted build.
            quarantine(&path)?;
        }
        fs::rename(staging.path(), &path)?;
        Ok(Self { path })
    }
}

fn quarantine(path: &Path) -> Result<()> {
    let parent = path.parent().context("quarantine parent")?;
    let directory = tempfile::Builder::new()
        .prefix(".quarantine-")
        .tempdir_in(parent)?
        .keep();
    fs::rename(
        path,
        directory.join(path.file_name().context("quarantine name")?),
    )?;
    Ok(())
}

fn collect_files(root: &Path, relative: &Path, files: &mut BTreeSet<PathBuf>) -> Result<()> {
    let directory = root.join(relative);
    if !directory.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let relative_path = path.strip_prefix(root)?.to_path_buf();
        if path.is_dir() {
            collect_files(root, &relative_path, files)?;
        } else if path.is_file() {
            files.insert(relative_path);
        }
    }
    Ok(())
}

fn required_ffi_inputs(package: &Path) -> Vec<PathBuf> {
    [
        package.join("Package.swift"),
        package.join("Sources/CutoutMobileFFI/cutout_mobile_ffi.swift"),
        package.join("cutout_mobile_ffiFFI.xcframework/Info.plist"),
        package.join("cutout_mobile_ffiFFI.xcframework/ios-arm64/libcutout_mobile_ffi.a"),
        package.join("cutout_mobile_ffiFFI.xcframework/ios-arm64/Headers/cutout_mobile_ffiFFI/cutout_mobile_ffiFFI.h"),
        package.join("cutout_mobile_ffiFFI.xcframework/ios-arm64/Headers/cutout_mobile_ffiFFI/module.modulemap"),
        package.join("cutout_mobile_ffiFFI.xcframework/ios-arm64-simulator/libcutout_mobile_ffi.a"),
        package.join("cutout_mobile_ffiFFI.xcframework/ios-arm64-simulator/Headers/cutout_mobile_ffiFFI/cutout_mobile_ffiFFI.h"),
        package.join("cutout_mobile_ffiFFI.xcframework/ios-arm64-simulator/Headers/cutout_mobile_ffiFFI/module.modulemap"),
        package.join("cutout_mobile_ffiFFI.xcframework/macos-arm64/libcutout_mobile_ffi.a"),
        package.join("cutout_mobile_ffiFFI.xcframework/macos-arm64/Headers/cutout_mobile_ffiFFI/cutout_mobile_ffiFFI.h"),
        package.join("cutout_mobile_ffiFFI.xcframework/macos-arm64/Headers/cutout_mobile_ffiFFI/module.modulemap"),
    ]
    .into()
}

fn ffi_output_hashes(package: &Path) -> Result<Value> {
    let mut inputs = BTreeSet::new();
    collect_files(package, Path::new("Sources"), &mut inputs)?;
    collect_files(
        package,
        Path::new("cutout_mobile_ffiFFI.xcframework"),
        &mut inputs,
    )?;
    // Require the known files, and also inventory additions SwiftPM could compile or link.
    for input in required_ffi_inputs(package).into_iter().chain([
        package.join(FFI_CHECKER),
        package.join("Sources/CutoutMobileFFI/CutoutArtifactIdentity.swift"),
    ]) {
        inputs.insert(input.strip_prefix(package)?.to_path_buf());
    }
    let mut hashes = serde_json::Map::new();
    for relative in inputs {
        let input = package.join(&relative);
        let bytes = fs::read(&input)
            .with_context(|| format!("reading Swift FFI artifact {}", input.display()))?;
        ensure!(
            !bytes.is_empty(),
            "empty Swift FFI artifact: {}",
            input.display()
        );
        hashes.insert(
            relative.to_string_lossy().into_owned(),
            Value::String(hex(Sha256::digest(bytes))),
        );
    }
    Ok(Value::Object(hashes))
}

fn write_ffi_receipt(package: &Path, source: &str) -> Result<()> {
    let receipt = serde_json::json!({
        "version": 2,
        "source": source,
        "outputs": ffi_output_hashes(package)?,
    });
    fs::write(package.join(FFI_RECEIPT), serde_json::to_vec(&receipt)?)?;
    Ok(())
}

/// Read-only so the SwiftPM/Xcode build-tool sandbox can run it on every build.
fn verify_ffi_receipt(package: &Path, source: &str) -> Result<String> {
    let bytes =
        fs::read(package.join(FFI_RECEIPT)).context("missing Swift FFI artifact receipt")?;
    let receipt: Value = serde_json::from_slice(&bytes)?;
    ensure!(
        receipt["version"] == 2,
        "unsupported Swift FFI artifact receipt version"
    );
    ensure!(
        receipt["source"] == source,
        "Swift FFI was built from different Rust inputs"
    );
    ensure!(
        receipt["outputs"] == ffi_output_hashes(package)?,
        "Swift FFI artifact contents changed after generation"
    );
    Ok(hex(Sha256::digest(bytes)))
}

fn check_swift_ffi_build(root: &Path, package: &Path, output: &Path) -> Result<()> {
    let selected = selected_ffi_generation(root)?;
    ensure!(
        fs::canonicalize(package)? == fs::canonicalize(selected)?,
        "Swift build graph has an obsolete Rust artifact; resolve the current package"
    );
    // Check the dependency selected by this build graph, not the mutable alias.
    let identity = verify_ffi_receipt(package, &source_fingerprint(root)?)
        .context("Swift FFI is stale or incomplete. Use `devenv shell -- cargo cutout swift -- ...` to prepare and build in one invocation")?;
    fs::create_dir_all(output)?;
    let path = output.join("CutoutVerifiedArtifact.swift");
    let source = format!(
        "// Verified by the Rust FFI build boundary.\nlet cutoutVerifiedArtifact = \"{identity}\"\n"
    );
    if fs::read_to_string(&path).ok().as_deref() != Some(source.as_str()) {
        fs::write(path, source)?;
    }
    Ok(())
}

fn verify_swift_ffi(package: &Path) -> Result<()> {
    verify_swift_ffi_files(package)?;
    if cfg!(target_os = "macos") {
        for slice in ["ios-arm64", "ios-arm64-simulator", "macos-arm64"] {
            let library = package.join(format!(
                "cutout_mobile_ffiFFI.xcframework/{slice}/libcutout_mobile_ffi.a"
            ));
            let output = Command::new("/usr/bin/lipo")
                .arg(&library)
                .arg("-archs")
                .output()
                .with_context(|| format!("listing architectures in {}", library.display()))?;
            ensure!(
                output.status.success(),
                "failed to inspect architectures in {}",
                library.display()
            );
            let architectures = String::from_utf8(output.stdout)
                .with_context(|| format!("decoding architectures in {}", library.display()))?;
            ensure!(
                architectures.trim() == "arm64",
                "{} has unexpected architectures: {}",
                library.display(),
                architectures.trim()
            );
        }
    }
    Ok(())
}

fn verify_swift_ffi_files(package: &Path) -> Result<()> {
    for input in required_ffi_inputs(package) {
        ensure!(
            input.is_file() && fs::metadata(&input)?.len() > 0,
            "missing or empty Swift FFI build input: {}",
            input.display()
        );
    }
    let manifest = fs::read_to_string(package.join("Package.swift"))?;
    ensure!(
        manifest.match_indices("Package").any(|(offset, _)| {
            manifest[offset + "Package".len()..]
                .trim_start()
                .starts_with('(')
        }),
        "invalid Swift FFI Package.swift"
    );
    Ok(())
}

fn trim_generated_sources(package: &Path) -> Result<()> {
    let mut pending = vec![package.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if matches!(
                path.extension().and_then(OsStr::to_str),
                Some("swift" | "h")
            ) || path.file_name() == Some(OsStr::new("module.modulemap"))
            {
                let text = fs::read_to_string(&path)?;
                let trimmed = text
                    .split('\n')
                    .map(|line| line.trim_end_matches([' ', '\t']))
                    .collect::<Vec<_>>()
                    .join("\n");
                fs::write(path, trimmed)?;
            }
        }
    }
    Ok(())
}

fn sort_xcframework_plist(package: &Path) -> Result<()> {
    const SORT_PLIST: &str = r#"
import plistlib, sys
path = sys.argv[1]
with open(path, "rb") as source:
    plist = plistlib.load(source)
plist["AvailableLibraries"].sort(key=lambda library: library["LibraryIdentifier"])
with open(path, "wb") as destination:
    plistlib.dump(plist, destination, sort_keys=False)
"#;
    run(
        command("python3")
            .args(["-c", SORT_PLIST])
            .arg(package.join("cutout_mobile_ffiFFI.xcframework/Info.plist")),
        "sort XCFramework metadata",
    )
}

fn plist_value(path: &Path, key: &str) -> Result<String> {
    let output = command("/usr/libexec/PlistBuddy")
        .args(["-c", &format!("Print {key}")])
        .arg(path)
        .output()?;
    ensure!(
        output.status.success(),
        "PlistBuddy failed for {}",
        path.display()
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn ensure_empty_wrapper(name: &str) -> Result<()> {
    ensure!(
        env::var_os(name).is_none_or(|value| value.is_empty()),
        "{name} must be disabled"
    );
    Ok(())
}

fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    command
        .env(
            "DEVELOPER_DIR",
            env::var_os("CUTOUT_DEVELOPER_DIR")
                .unwrap_or_else(|| "/Applications/Xcode-beta.app/Contents/Developer".into()),
        )
        .env_remove("SDKROOT");
    command
}

fn run(command: &mut Command, description: &str) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("failed to start command to {description}"))?;
    ensure_success(status, description)
}

fn ensure_success(status: ExitStatus, description: &str) -> Result<()> {
    ensure!(status.success(), "failed to {description}: {status}");
    Ok(())
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().fold(String::new(), |mut hex, byte| {
        write!(hex, "{byte:02x}").expect("writing to a String cannot fail");
        hex
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_build_commands_preserve_native_arguments() {
        let args = ["swift", "--", "test", "--package-path", "a package"].map(str::to_owned);
        assert_eq!(
            parse_cli(&args).unwrap(),
            DevCommand::Swift(args[2..].to_vec())
        );
        let args = ["xcodebuild", "--", "-scheme", "An App", "build"].map(str::to_owned);
        assert_eq!(
            parse_cli(&args).unwrap(),
            DevCommand::Xcodebuild(args[2..].to_vec())
        );
        assert!(parse_cli(&["swift".into(), "test".into()]).is_err());
    }

    #[test]
    fn apple_build_commands_reject_bypassing_the_build_graph() {
        for (tool, bypass) in [
            ("swift", "--skip-build"),
            ("xcodebuild", "test-without-building"),
        ] {
            let error =
                build_apple_client(Path::new("must-not-be-created"), tool, &[bypass.into()])
                    .unwrap_err();
            assert!(error.to_string().contains("requires a build"));
        }
    }

    fn fixture_sources(root: &Path) {
        fs::create_dir_all(root.join("crates/fixture/src")).unwrap();
        for name in [
            "Cargo.lock",
            "Cargo.toml",
            "rust-toolchain.toml",
            "crates/fixture/Cargo.toml",
            "crates/fixture/src/lib.rs",
        ] {
            fs::write(root.join(name), "original").unwrap();
        }
    }

    fn fixture_artifact(package: &Path, source: &str, build: &str) {
        for path in required_ffi_inputs(package).into_iter().chain([
            package.join(FFI_CHECKER),
            package.join("Sources/CutoutMobileFFI/CutoutArtifactIdentity.swift"),
        ]) {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, build).unwrap();
        }
        write_ffi_receipt(package, source).unwrap();
    }

    #[test]
    fn ffi_source_snapshot_is_independent_of_later_worktree_edits() {
        let root = env::temp_dir().join(format!("cutout-ffi-snapshot-{}", std::process::id()));
        fixture_sources(&root);
        let expected = source_fingerprint(&root).unwrap();
        let snapshot = FfiSourceSnapshot::capture(&root, &expected).unwrap();
        fs::write(root.join("crates/fixture/src/lib.rs"), "later edit").unwrap();
        assert_eq!(source_fingerprint(&snapshot.path).unwrap(), expected);
        assert_ne!(source_fingerprint(&root).unwrap(), expected);
        assert!(FfiSourceSnapshot::capture(&root, &expected).is_err());
        fs::write(root.join("crates/fixture/src/lib.rs"), "original").unwrap();
        let reused = FfiSourceSnapshot::capture(&root, &expected).unwrap();
        assert_eq!(reused.path, snapshot.path);
        fs::remove_dir_all(snapshot.path.parent().unwrap()).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ffi_source_snapshot_repairs_poisoned_lockfile_and_copies_cargo_config() {
        let root = env::temp_dir().join(format!("cutout-ffi-config-{}", std::process::id()));
        fixture_sources(&root);
        fs::create_dir_all(root.join(".cargo")).unwrap();
        for config in [".cargo/config", ".cargo/config.toml"] {
            fs::write(root.join(config), config).unwrap();
        }
        let expected = source_fingerprint(&root).unwrap();
        let first = FfiSourceSnapshot::capture(&root, &expected).unwrap();
        for config in [".cargo/config", ".cargo/config.toml"] {
            assert_eq!(
                fs::read(first.path.join(config)).unwrap(),
                config.as_bytes()
            );
            fs::write(root.join(config), "changed").unwrap();
            assert_ne!(source_fingerprint(&root).unwrap(), expected);
            fs::write(root.join(config), config).unwrap();
        }
        fs::write(first.path.join("Cargo.lock"), "poisoned").unwrap();
        let repaired = FfiSourceSnapshot::capture(&root, &expected).unwrap();
        assert_eq!(repaired.path, first.path);
        assert_eq!(source_fingerprint(&repaired.path).unwrap(), expected);
        assert!(
            fs::read_dir(repaired.path.parent().unwrap())
                .unwrap()
                .any(|entry| entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".quarantine-"))
        );
        fs::remove_dir_all(repaired.path.parent().unwrap()).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ffi_publication_repairs_corrupt_generation_without_rewriting_valid_one() {
        let root = env::temp_dir().join(format!("cutout-ffi-repair-{}", std::process::id()));
        fixture_sources(&root);
        let source = source_fingerprint(&root).unwrap();
        let staged = root.join("staged");
        fixture_artifact(&staged, &source, "build-a");
        let generation = publish_swift_ffi(&root, &staged, &source).unwrap();
        let selector = fs::read(root.join(GENERATED_PACKAGE).join("Package.swift")).unwrap();
        fixture_artifact(&staged, &source, "build-a");
        publish_swift_ffi(&root, &staged, &source).unwrap();
        assert!(
            !generation
                .parent()
                .unwrap()
                .read_dir()
                .unwrap()
                .any(|entry| {
                    entry
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".quarantine-")
                })
        );
        fs::write(
            generation.join("Sources/CutoutMobileFFI/cutout_mobile_ffi.swift"),
            "bad",
        )
        .unwrap();
        fixture_artifact(&staged, &source, "build-a");
        assert_eq!(
            publish_swift_ffi(&root, &staged, &source).unwrap(),
            generation
        );
        verify_ffi_receipt(&generation, &source).unwrap();
        assert_eq!(
            fs::read(root.join(GENERATED_PACKAGE).join("Package.swift")).unwrap(),
            selector
        );
        assert!(
            generation
                .parent()
                .unwrap()
                .read_dir()
                .unwrap()
                .any(|entry| {
                    entry
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".quarantine-")
                })
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn ffi_publication_preserves_a_pinned_concurrent_reader() {
        let root = env::temp_dir().join(format!("cutout-ffi-publish-{}", std::process::id()));
        fixture_sources(&root);
        let source = source_fingerprint(&root).unwrap();
        let staged = root.join("staged");
        fixture_artifact(&staged, &source, "build-a");
        let first = publish_swift_ffi(&root, &staged, &source).unwrap();
        let pinned = selected_ffi_generation(&root).unwrap();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let reader_root = root.clone();
        let reader = std::thread::spawn(move || {
            let before = fs::read(pinned.join(FFI_RECEIPT)).unwrap();
            check_swift_ffi_build(&reader_root, &pinned, &reader_root.join("plugin-output"))
                .unwrap();
            ready_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            verify_ffi_receipt(&pinned, &source_fingerprint(&reader_root).unwrap()).unwrap();
            assert_eq!(fs::read(pinned.join(FFI_RECEIPT)).unwrap(), before);
        });
        ready_rx.recv().unwrap();
        fixture_artifact(&staged, &source, "build-a");
        fs::write(
            staged.join("Sources/CutoutMobileFFI/CutoutArtifactIdentity.swift"),
            "new artifact, same source and build",
        )
        .unwrap();
        write_ffi_receipt(&staged, &source).unwrap();
        let second = publish_swift_ffi(&root, &staged, &source).unwrap();
        assert_ne!(first, second);
        assert_eq!(selected_ffi_generation(&root).unwrap(), second);
        release_tx.send(()).unwrap();
        reader.join().unwrap();
        // A failed/incomplete generation must leave the selection untouched.
        fs::create_dir_all(&staged).unwrap();
        assert!(publish_swift_ffi(&root, &staged, &source).is_err());
        assert_eq!(selected_ffi_generation(&root).unwrap(), second);
        // Validation must inspect the pinned dependency, even when current is valid.
        fs::write(
            first.join("Sources/CutoutMobileFFI/CutoutArtifactIdentity.swift"),
            "bad",
        )
        .unwrap();
        assert!(check_swift_ffi_build(&root, &first, &root.join("plugin-output")).is_err());
        assert!(check_swift_ffi_build(&root, &second, &root.join("plugin-output")).is_ok());
        // An old graph must not pass after preparation selects a different SDK or flags.
        fixture_artifact(&staged, &source, "build-b");
        publish_swift_ffi(&root, &staged, &source).unwrap();
        assert!(check_swift_ffi_build(&root, &second, &root.join("plugin-output")).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn swift_ffi_package_matches_the_swift_package_dependency() {
        assert_eq!(GENERATED_PACKAGE, "target/swift-ffi");
    }

    #[test]
    fn swift_ffi_inputs_do_not_require_intel_slices() {
        assert!(
            required_ffi_inputs(Path::new(GENERATED_PACKAGE))
                .iter()
                .all(|path| !path.to_string_lossy().contains("x86_64"))
        );
    }

    #[test]
    fn swift_ffi_rejects_missing_empty_and_invalid_inputs() {
        let package =
            env::temp_dir().join(format!("cutout-dev-ffi-inputs-test-{}", std::process::id()));
        let inputs = required_ffi_inputs(&package);
        for input in &inputs {
            fs::create_dir_all(input.parent().unwrap()).unwrap();
            fs::write(input, "generated\n").unwrap();
        }
        let manifest = package.join("Package.swift");
        fs::write(
            &manifest,
            "let package = Package (name: \"CutoutMobileFFI\")\n",
        )
        .unwrap();
        verify_swift_ffi_files(&package).unwrap();
        for input in &inputs {
            let contents = fs::read(input).unwrap();
            fs::remove_file(input).unwrap();
            assert!(
                verify_swift_ffi_files(&package).is_err(),
                "{}",
                input.display()
            );
            fs::write(input, "").unwrap();
            assert!(
                verify_swift_ffi_files(&package).is_err(),
                "{}",
                input.display()
            );
            fs::write(input, contents).unwrap();
        }
        fs::write(&manifest, "not a package manifest\n").unwrap();
        assert!(verify_swift_ffi_files(&package).is_err());
        fs::remove_dir_all(package).unwrap();
    }

    #[test]
    fn ffi_receipt_rejects_stale_sources_and_changed_artifacts() {
        let package = env::temp_dir().join(format!("cutout-ffi-receipt-{}", std::process::id()));
        for path in required_ffi_inputs(&package).into_iter().chain([
            package.join(FFI_CHECKER),
            package.join("Sources/CutoutMobileFFI/CutoutArtifactIdentity.swift"),
        ]) {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "original").unwrap();
        }
        assert!(verify_ffi_receipt(&package, "rust-a").is_err());
        write_ffi_receipt(&package, "rust-a").unwrap();
        let original = verify_ffi_receipt(&package, "rust-a").unwrap();
        assert!(verify_ffi_receipt(&package, "rust-b").is_err());
        let added = package.join("Sources/CutoutMobileFFI/Unexpected.swift");
        fs::write(&added, "let unexpected = true").unwrap();
        assert!(
            verify_ffi_receipt(&package, "rust-a").is_err(),
            "new compiled source must fail"
        );
        fs::remove_file(added).unwrap();
        let library =
            package.join("cutout_mobile_ffiFFI.xcframework/macos-arm64/libcutout_mobile_ffi.a");
        fs::write(&library, "modified").unwrap();
        assert!(
            verify_ffi_receipt(&package, "rust-a").is_err(),
            "same-size archive replacement must fail"
        );
        fs::write(&library, "original").unwrap();
        write_ffi_receipt(&package, "rust-b").unwrap();
        assert_ne!(original, verify_ffi_receipt(&package, "rust-b").unwrap());
        fs::remove_file(&library).unwrap();
        assert!(verify_ffi_receipt(&package, "rust-b").is_err());
        fs::remove_dir_all(package).unwrap();
    }

    #[test]
    fn ios_deploy_accepts_launch_arguments_after_separator() {
        let args = ["ios", "deploy", "--", "--launch-smoke"].map(str::to_owned);

        assert_eq!(
            parse_cli(&args).unwrap(),
            DevCommand::IosDeploy(vec!["--launch-smoke".to_owned()])
        );
    }

    #[test]
    fn simulator_accepts_the_aero_settings_scenario() {
        let args = ["simulator", "aero-settings"].map(str::to_owned);

        assert_eq!(parse_cli(&args).unwrap(), DevCommand::AeroSettingsSimulator);
    }

    #[test]
    fn ios_metadata_rejects_missing_and_incorrect_values() {
        let valid = serde_json::json!({
            "CFBundleDisplayName": "CutOut",
            "NSBluetoothAlwaysUsageDescription": "CutOut uses Bluetooth to read live vehicle telemetry.",
            "UIDeviceFamily": [1],
            "UISupportedInterfaceOrientations": [
                "UIInterfaceOrientationPortrait",
                "UIInterfaceOrientationLandscapeLeft",
                "UIInterfaceOrientationLandscapeRight"
            ]
        });
        verify_ios_metadata(&valid).unwrap();
        for key in valid.as_object().unwrap().keys() {
            let mut changed = valid.clone();
            changed.as_object_mut().unwrap().remove(key);
            assert!(verify_ios_metadata(&changed).is_err(), "missing {key}");
            changed[key] = serde_json::json!("incorrect");
            assert!(verify_ios_metadata(&changed).is_err(), "incorrect {key}");
        }
        assert_eq!(
            parse_cli(&["ios".into(), "verify-app".into(), "Cutout App.app".into()]).unwrap(),
            DevCommand::IosVerifyApp(PathBuf::from("Cutout App.app"))
        );
    }

    #[test]
    fn capture_selection_filters_sorts_and_limits_device_files() {
        let files = serde_json::json!({"result": {"files": [
            {"name": "cutout-btle-capture-old.jsonl", "metadata": {"lastModDate": "2026-01-01"}},
            {"name": "unrelated.jsonl", "metadata": {"lastModDate": "2026-12-01"}},
            {"name": "cutout-btle-capture-new.jsonl", "metadata": {"lastModDate": "2026-02-01"}},
            {"name": "cutout-btle-capture-../../bad.jsonl"},
            {"name": "cutout-btle-capture-backup.txt"}
        ]}});
        assert_eq!(
            capture_names(&files, 1).unwrap(),
            ["cutout-btle-capture-new.jsonl"]
        );
        assert_eq!(
            capture_names(&files, 5).unwrap(),
            [
                "cutout-btle-capture-new.jsonl",
                "cutout-btle-capture-old.jsonl"
            ]
        );
        assert!(capture_names(&serde_json::json!({}), 5).is_err());
        assert!(
            capture_names(&serde_json::json!({"result": {"files": []}}), 5)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            parse_cli(&["ios".into(), "captures".into()]).unwrap(),
            DevCommand::IosCaptures
        );
    }

    #[test]
    fn ios_signing_requires_a_team_and_forwards_bundle_id() {
        assert!(ios_signing_arguments(None, None).is_err());
        assert_eq!(
            ios_signing_arguments(Some("TEAM"), Some("org.example.cutout")).unwrap(),
            [
                "CODE_SIGNING_ALLOWED=YES",
                "CODE_SIGNING_REQUIRED=YES",
                "CODE_SIGN_STYLE=Automatic",
                "CODE_SIGN_IDENTITY=Apple Development",
                "DEVELOPMENT_TEAM=TEAM",
                "PRODUCT_BUNDLE_IDENTIFIER=org.example.cutout",
            ]
        );
    }

    #[test]
    fn device_list_output_is_unique_between_processes() {
        let root = Path::new("workspace");

        assert_ne!(
            device_list_output_path(root, 41),
            device_list_output_path(root, 42)
        );
    }

    #[test]
    fn usage_rejects_failed_commands() {
        let failed = if cfg!(unix) {
            Command::new("false").status().unwrap()
        } else {
            Command::new("cmd").args(["/C", "exit 1"]).status().unwrap()
        };
        assert!(ensure_success(failed, "test command").is_err());
    }

    #[test]
    fn fingerprint_changes_with_rust_inputs() {
        let root = env::temp_dir().join(format!("cutout-dev-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for directory in [
            "crates/cutout-core/src",
            "crates/cutout-mobile-ffi/src",
            "crates/cutout-protocols/src",
            "crates/cutout-ride-maps/src",
            "crates/libcutout-persistence/src",
            "crates/cutout-uniffi-bindgen/src",
        ] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        for file in [
            "Cargo.lock",
            "Cargo.toml",
            "rust-toolchain.toml",
            "crates/cutout-core/Cargo.toml",
            "crates/cutout-mobile-ffi/Cargo.toml",
            "crates/cutout-protocols/Cargo.toml",
            "crates/cutout-ride-maps/Cargo.toml",
            "crates/libcutout-persistence/Cargo.toml",
            "crates/cutout-uniffi-bindgen/Cargo.toml",
            "crates/cutout-core/src/lib.rs",
        ] {
            fs::write(root.join(file), "original\n").unwrap();
        }

        for file in [
            "crates/cutout-core/src/lib.rs",
            "crates/cutout-ride-maps/src/lib.rs",
            "crates/libcutout-persistence/src/lib.rs",
            "crates/cutout-uniffi-bindgen/src/main.rs",
            "crates/libcutout-persistence/build.rs",
        ] {
            let before = source_fingerprint(&root).unwrap();
            fs::write(root.join(file), "changed\n").unwrap();
            assert_ne!(before, source_fingerprint(&root).unwrap(), "{file}");
        }
        let before = source_fingerprint(&root).unwrap();
        let artifact = root.join("crates/cutout-mobile-ffi/generated");
        fs::create_dir_all(&artifact).unwrap();
        fs::write(artifact.join("lib.a"), "generated").unwrap();
        assert_eq!(before, source_fingerprint(&root).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn swift_ffi_lock_is_exclusive_and_released_with_its_handle() {
        let root =
            env::temp_dir().join(format!("cutout-dev-stale-lock-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let lock = lock_swift_ffi(&root).unwrap();
        let contender = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join(SWIFT_FFI_LOCK))
            .unwrap();
        assert!(matches!(
            contender.try_lock(),
            Err(fs::TryLockError::WouldBlock)
        ));
        drop(lock);
        contender.try_lock().unwrap();
        drop(contender);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn ffi_child_keeps_lock_after_coordinator_is_killed() {
        use std::io::{BufRead, Write};

        let root = env::var_os("CUTOUT_FFI_LOCK_TEST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                env::temp_dir().join(format!("cutout-ffi-orphan-{}", std::process::id()))
            });
        if env::var_os("CUTOUT_FFI_LOCK_TEST_CHILD").is_some() {
            let lock = lock_swift_ffi(&root).unwrap();
            let mut child = Command::new("sleep")
                .arg("30")
                .stdin(lock.try_clone().unwrap())
                .spawn()
                .unwrap();
            println!("{}", child.id());
            std::io::stdout().flush().unwrap();
            child.wait().unwrap();
            return;
        }

        let mut coordinator = Command::new(env::current_exe().unwrap())
            .args([
                "--exact",
                "tests::ffi_child_keeps_lock_after_coordinator_is_killed",
                "--nocapture",
            ])
            .env("CUTOUT_FFI_LOCK_TEST_CHILD", "1")
            .env("CUTOUT_FFI_LOCK_TEST_ROOT", &root)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut output = std::io::BufReader::new(coordinator.stdout.take().unwrap());
        let mut line = String::new();
        let child_pid = loop {
            line.clear();
            assert_ne!(
                output.read_line(&mut line).unwrap(),
                0,
                "coordinator exited early"
            );
            if let Ok(pid) = line.trim().parse::<u32>() {
                break pid;
            }
        };
        assert!(
            Command::new("kill")
                .args(["-KILL", &coordinator.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        coordinator.wait().unwrap();
        let contender = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join(SWIFT_FFI_LOCK))
            .unwrap();
        assert!(matches!(
            contender.try_lock(),
            Err(fs::TryLockError::WouldBlock)
        ));
        assert!(
            Command::new("kill")
                .args(["-TERM", &child_pid.to_string()])
                .status()
                .unwrap()
                .success()
        );
        fs::remove_dir_all(root).unwrap();
    }
}
