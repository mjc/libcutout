use std::{
    collections::BTreeSet,
    env,
    ffi::OsStr,
    fmt::Write,
    fs,
    io::{ErrorKind, Write as _},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail, ensure};
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

const GENERATED_PACKAGE: &str = "target/swift-ffi/CutoutMobileFFI";
const CARGO_SWIFT_PACKAGE: &str = "crates/cutout-mobile-ffi/CutoutMobileFFI";
const SWIFT_FFI_LOCK: &str = "target/swift-ffi/.cutout-swift-ffi.lock";

struct SwiftFfiLock {
    path: PathBuf,
}

impl SwiftFfiLock {
    fn acquire(root: &Path) -> Result<Self> {
        let path = root.join(SWIFT_FFI_LOCK);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let started = Instant::now();
        loop {
            match fs::create_dir(&path) {
                Ok(()) => {
                    let owner = path.join("owner");
                    let mut lock = fs::File::create(&owner)?;
                    writeln!(lock, "{}", std::process::id())?;
                    return Ok(Self { path });
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    // The directory is the atomic ownership token. An owner file may be absent
                    // briefly while the winner records its PID; never reclaim that state.
                    if !path.is_dir() {
                        let owner = fs::read_to_string(&path).unwrap_or_default();
                        let owner_pid = owner.trim().parse::<u32>().ok();
                        if let Some(pid) = owner_pid {
                            if process_is_alive(pid) {
                                ensure!(
                                    started.elapsed() < Duration::from_secs(120),
                                    "timed out waiting for Swift FFI generation lock {}",
                                    path.display()
                                );
                                thread::sleep(Duration::from_millis(100));
                                continue;
                            }
                            if fs::read_to_string(&path).unwrap_or_default() == owner {
                                let _ = fs::remove_file(&path);
                                continue;
                            }
                        }
                        return Err(anyhow!(
                            "malformed Swift FFI lock file exists at {}",
                            path.display()
                        ));
                    }
                    let owner_path = path.join("owner");
                    let owner = fs::read_to_string(&owner_path).unwrap_or_default();
                    let owner_pid = owner.trim().parse::<u32>().ok();
                    if let Some(pid) = owner_pid {
                        if !process_is_alive(pid)
                            && fs::read_to_string(&owner_path).unwrap_or_default() == owner
                        {
                            let _ = fs::remove_file(&owner_path);
                            let _ = fs::remove_dir(&path);
                            continue;
                        }
                    }
                    ensure!(
                        started.elapsed() < Duration::from_secs(120),
                        "timed out waiting for Swift FFI generation lock {}",
                        path.display()
                    );
                    thread::sleep(Duration::from_millis(100));
                }
                Err(error) => return Err(error).context("acquire Swift FFI generation lock"),
            }
        }
    }
}

fn process_is_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

impl Drop for SwiftFfiLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.path.join("owner"));
        let _ = fs::remove_dir(&self.path);
    }
}

#[derive(Debug, Eq, PartialEq)]
enum DevCommand {
    AeroSettingsSimulator,
    SwiftFfi,
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
            "usage: cutout-dev simulator aero-settings | cutout-dev swift-ffi | cutout-dev ios deploy [-- <launch args>...] | cutout-dev ios verify-app <app bundle> | cutout-dev ios captures"
        ),
    }
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
    let _lock = SwiftFfiLock::acquire(root)?;
    let package = root.join(GENERATED_PACKAGE);
    let expected = source_fingerprint(root)?;
    let current = fs::read_to_string(package.join(".cutout-source.sha256"))
        .unwrap_or_default()
        .trim()
        .to_owned();
    if current == expected && verify_swift_ffi(&package).is_ok() {
        return Ok(());
    }

    eprintln!("Regenerating stale Swift FFI artifact.");
    regenerate_swift_ffi(root, &package)?;
    fs::write(
        package.join(".cutout-source.sha256"),
        format!("{expected}\n"),
    )?;
    verify_swift_ffi(&package)?;
    Ok(())
}

fn regenerate_swift_ffi(root: &Path, package: &Path) -> Result<()> {
    ensure!(
        cfg!(target_os = "macos"),
        "Swift FFI artifact is stale or missing; regenerate it on macOS with `cargo cutout swift-ffi` before using Swift builds on this host"
    );
    ensure_empty_wrapper("RUSTC_WRAPPER")?;
    ensure_empty_wrapper("RUSTC_WORKSPACE_WRAPPER")?;

    let cargo_package = root.join(CARGO_SWIFT_PACKAGE);
    let backup = package.with_file_name(format!(".CutoutMobileFFI.backup.{}", std::process::id()));
    let cargo_backup = cargo_package.with_file_name(format!(
        ".CutoutMobileFFI.cargo-backup.{}",
        std::process::id()
    ));
    ensure!(
        !backup.exists(),
        "generated-package backup already exists: {}",
        backup.display()
    );
    ensure!(
        !cargo_backup.exists(),
        "cargo-swift backup already exists: {}",
        cargo_backup.display()
    );
    if let Some(parent) = package.parent() {
        fs::create_dir_all(parent)?;
    }
    if package.exists() {
        fs::rename(package, &backup)
            .with_context(|| format!("backing up {}", package.display()))?;
    }
    if cargo_package.exists() {
        fs::rename(&cargo_package, &cargo_backup)
            .with_context(|| format!("backing up {}", cargo_package.display()))?;
    }

    let result = run(
        command("cargo")
            .current_dir(root.join("crates/cutout-mobile-ffi"))
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
                "--skip-toolchains-check",
                "--accept-all",
                "--swift-tools-version",
                "6.0",
                "--silent",
            ]),
        "generate Swift FFI package",
    )
    .and_then(|()| normalize_xcframework_to_arm64(&cargo_package))
    .and_then(|()| sort_xcframework_plist(&cargo_package))
    .and_then(|()| trim_generated_sources(&cargo_package))
    .and_then(|()| {
        fs::rename(&cargo_package, package).with_context(|| {
            format!(
                "moving generated Swift FFI package from {} to {}",
                cargo_package.display(),
                package.display()
            )
        })
    })
    .and_then(|()| verify_swift_ffi(package));

    match result {
        Ok(()) => {
            if backup.exists() {
                fs::remove_dir_all(&backup)?;
            }
            if cargo_backup.exists() {
                fs::remove_dir_all(&cargo_backup)?;
            }
            Ok(())
        }
        Err(error) => {
            if package.exists() {
                fs::remove_dir_all(package)?;
            }
            if backup.exists() {
                fs::rename(&backup, package)?;
            }
            if cargo_package.exists() {
                fs::remove_dir_all(&cargo_package)?;
            }
            if cargo_backup.exists() {
                fs::rename(&cargo_backup, cargo_package)?;
            }
            Err(error)
        }
    }
}

/// Keep the generated package limited to architectures used by this project.
/// cargo-swift emits universal Intel slices by default; no supported workflow
/// here needs x86_64, and retaining those slices makes generation depend on
/// an Intel macOS SDK being available.
fn normalize_xcframework_to_arm64(package: &Path) -> Result<()> {
    ensure!(
        cfg!(target_os = "macos"),
        "XCFramework normalization requires macOS"
    );
    let xcframework = package.join("cutout_mobile_ffiFFI.xcframework");
    for (source_name, target_name) in [
        ("ios-arm64_x86_64-simulator", "ios-arm64-simulator"),
        ("macos-arm64_x86_64", "macos-arm64"),
    ] {
        let source = xcframework.join(source_name);
        let target = xcframework.join(target_name);
        if source.exists() {
            fs::rename(&source, &target)
                .with_context(|| format!("renaming {source_name} XCFramework slice"))?;
        }
        let library = target.join("libcutout_mobile_ffi.a");
        let arm64 = target.join("libcutout_mobile_ffi.arm64.a");
        run(
            command("/usr/bin/lipo")
                .args(["-thin", "arm64", "-output"])
                .arg(&arm64)
                .arg(&library),
            "thin Swift FFI library to arm64",
        )?;
        fs::rename(arm64, library).context("install arm64 Swift FFI library")?;
    }

    const NORMALIZE_PLIST: &str = r#"
import plistlib, sys
path = sys.argv[1]
with open(path, "rb") as source:
    plist = plistlib.load(source)
for library in plist["AvailableLibraries"]:
    identifiers = {
        "ios-arm64_x86_64-simulator": "ios-arm64-simulator",
        "macos-arm64_x86_64": "macos-arm64",
    }
    library["LibraryIdentifier"] = identifiers.get(
        library["LibraryIdentifier"], library["LibraryIdentifier"]
    )
    library["SupportedArchitectures"] = ["arm64"]
with open(path, "wb") as destination:
    plistlib.dump(plist, destination, sort_keys=False)
"#;
    run(
        command("python3")
            .args(["-c", NORMALIZE_PLIST])
            .arg(xcframework.join("Info.plist")),
        "normalize Swift FFI XCFramework metadata",
    )
}

fn deploy_ios(root: &Path, launch_args: &[String]) -> Result<()> {
    ensure!(
        cfg!(target_os = "macos"),
        "iPhone deployment requires macOS"
    );
    ensure_swift_ffi(root)?;

    let device = match env::var("CUTOUT_IOS_DEVICE_UDID") {
        Ok(device) => device,
        Err(_) => discover_ios_device(root)?,
    };
    let derived_data = root.join("target/xcode-device-signed");
    let product = derived_data.join("Build/Products/Debug-iphoneos/CutoutApp.app");
    if product.exists() {
        fs::remove_dir_all(&product)?;
    }

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
    let mut files = BTreeSet::from([
        PathBuf::from("Cargo.lock"),
        PathBuf::from("Cargo.toml"),
        PathBuf::from("rust-toolchain.toml"),
    ]);
    // Include workspace sources, including transitive FFI dependencies and the
    // generator itself. Generated packages and build artifacts are not inputs.
    for entry in fs::read_dir(root.join("crates"))? {
        let path = entry?.path();
        if !path.join("Cargo.toml").is_file() {
            continue;
        }
        let relative = path.strip_prefix(root)?;
        for directory in ["src", "registry"] {
            collect_files(root, &relative.join(directory), &mut files)?;
        }
        for name in ["Cargo.toml", "build.rs", "uniffi.toml"] {
            if path.join(name).is_file() {
                files.insert(relative.join(name));
            }
        }
    }

    let mut aggregate = Sha256::new();
    for relative in files {
        let bytes = fs::read(root.join(&relative))
            .with_context(|| format!("reading fingerprint input {}", relative.display()))?;
        let file_hash = hex(Sha256::digest(bytes));
        aggregate.update(relative.as_os_str().as_encoded_bytes());
        aggregate.update(b"  ");
        aggregate.update(file_hash.as_bytes());
        aggregate.update(b"  ");
        aggregate.update(relative.as_os_str().as_encoded_bytes());
        aggregate.update(b"\n");
    }
    Ok(hex(aggregate.finalize()))
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
    fn swift_ffi_package_matches_the_swift_package_dependency() {
        assert_eq!(GENERATED_PACKAGE, "target/swift-ffi/CutoutMobileFFI");
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
    fn swift_ffi_lock_reclaims_a_dead_owner() {
        let root =
            env::temp_dir().join(format!("cutout-dev-stale-lock-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let lock_path = root.join(SWIFT_FFI_LOCK);
        fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
        // A PID outside the host's normal process range gives a deterministic dead owner.
        let exited_pid = 2_147_483_647;
        fs::create_dir(&lock_path).unwrap();
        fs::write(lock_path.join("owner"), format!("{exited_pid}\n")).unwrap();

        let lock = SwiftFfiLock::acquire(&root).expect("dead lock owner is reclaimed");
        assert_eq!(
            fs::read_to_string(lock_path.join("owner")).unwrap().trim(),
            std::process::id().to_string()
        );
        drop(lock);
        assert!(!lock_path.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
