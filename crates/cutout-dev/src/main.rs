use std::{
    collections::BTreeSet,
    env,
    ffi::OsStr,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

use anyhow::{Context, Result, bail, ensure};
use cutout_core::{
    AeroAngleAdjustment, AeroPwmPercent, AeroSpeedSetting, DeviceCommand, LightState,
    MonotonicTimestamp, PedalMode, RideOperatingState,
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
        let mut lock = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| {
                format!(
                    "acquire Swift FFI generation lock {}; another generation may be running",
                    path.display()
                )
            })?;
        use std::io::Write as _;
        writeln!(lock, "{}", std::process::id())?;
        Ok(Self { path })
    }
}

impl Drop for SwiftFfiLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug, Eq, PartialEq)]
enum DevCommand {
    AeroSettingsSimulator,
    SwiftFfi,
    IosDeploy(Vec<String>),
}

fn main() -> Result<()> {
    let root = workspace_root();
    let args = env::args().skip(1).collect::<Vec<_>>();
    match parse_cli(&args)? {
        DevCommand::AeroSettingsSimulator => run_aero_settings_simulator(),
        DevCommand::SwiftFfi => ensure_swift_ffi(&root),
        DevCommand::IosDeploy(launch_args) => deploy_ios(&root, &launch_args),
    }
}

fn parse_cli(args: &[String]) -> Result<DevCommand> {
    match args {
        [simulator, scenario] if simulator == "simulator" && scenario == "aero-settings" => {
            Ok(DevCommand::AeroSettingsSimulator)
        }
        [command] if command == "swift-ffi" => Ok(DevCommand::SwiftFfi),
        [ios, deploy] if ios == "ios" && deploy == "deploy" => {
            Ok(DevCommand::IosDeploy(Vec::new()))
        }
        [ios, deploy, separator, launch_args @ ..]
            if ios == "ios" && deploy == "deploy" && separator == "--" =>
        {
            Ok(DevCommand::IosDeploy(launch_args.to_vec()))
        }
        _ => bail!(
            "usage: cutout-dev simulator aero-settings | cutout-dev swift-ffi | cutout-dev ios deploy [-- <launch args>...]"
        ),
    }
}

fn run_aero_settings_simulator() -> Result<()> {
    let commands = [
        DeviceCommand::SetAeroTiltbackSpeed(
            AeroSpeedSetting::new(53).context("53 km/h is a valid Aero tiltback speed")?,
        ),
        DeviceCommand::SetAeroPwmPercent(
            AeroPwmPercent::new(64).context("64% is a valid Aero PWM setting")?,
        ),
        DeviceCommand::SetAeroAlarmSpeed(
            AeroSpeedSetting::new(56).context("56 km/h is a valid Aero alarm speed")?,
        ),
        DeviceCommand::SetAeroAngleAdjustment(
            AeroAngleAdjustment::new(-12).context("-1.2 degrees is a valid Aero angle")?,
        ),
        DeviceCommand::SetPedalMode(PedalMode::Hard),
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
    build.arg("build");
    run(&mut build, "build signed iPhone app")?;
    ensure!(
        product.is_dir(),
        "Xcode did not produce {}",
        product.display()
    );

    let bundle_id = plist_value(&product.join("Info.plist"), ":CFBundleIdentifier")?;
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
        PathBuf::from("crates/cutout-core/Cargo.toml"),
        PathBuf::from("crates/cutout-mobile-ffi/Cargo.toml"),
        PathBuf::from("crates/cutout-protocols/Cargo.toml"),
    ]);
    for directory in [
        "crates/cutout-core/src",
        "crates/cutout-mobile-ffi/src",
        "crates/cutout-protocols/src",
        "crates/cutout-protocols/registry",
    ] {
        collect_files(root, Path::new(directory), &mut files)?;
    }
    for optional in [
        "crates/cutout-protocols/build.rs",
        "crates/cutout-mobile-ffi/uniffi.toml",
    ] {
        if root.join(optional).is_file() {
            files.insert(optional.into());
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
        package.join("cutout_mobile_ffiFFI.xcframework/ios-arm64_x86_64-simulator/libcutout_mobile_ffi.a"),
        package.join("cutout_mobile_ffiFFI.xcframework/ios-arm64_x86_64-simulator/Headers/cutout_mobile_ffiFFI/cutout_mobile_ffiFFI.h"),
        package.join("cutout_mobile_ffiFFI.xcframework/ios-arm64_x86_64-simulator/Headers/cutout_mobile_ffiFFI/module.modulemap"),
        package.join("cutout_mobile_ffiFFI.xcframework/macos-arm64_x86_64/libcutout_mobile_ffi.a"),
        package.join("cutout_mobile_ffiFFI.xcframework/macos-arm64_x86_64/Headers/cutout_mobile_ffiFFI/cutout_mobile_ffiFFI.h"),
        package.join("cutout_mobile_ffiFFI.xcframework/macos-arm64_x86_64/Headers/cutout_mobile_ffiFFI/module.modulemap"),
    ]
    .into()
}

fn verify_swift_ffi(package: &Path) -> Result<()> {
    for input in required_ffi_inputs(package) {
        ensure!(
            input.is_file(),
            "missing Swift FFI build input: {}",
            input.display()
        );
    }
    if cfg!(target_os = "macos") {
        for (slice, architectures) in [
            ("ios-arm64", &["arm64"][..]),
            ("ios-arm64_x86_64-simulator", &["arm64", "x86_64"][..]),
            ("macos-arm64_x86_64", &["arm64", "x86_64"][..]),
        ] {
            let library = package.join(format!(
                "cutout_mobile_ffiFFI.xcframework/{slice}/libcutout_mobile_ffi.a"
            ));
            for architecture in architectures {
                let status = Command::new("/usr/bin/lipo")
                    .arg(&library)
                    .args(["-verify_arch", architecture])
                    .status()?;
                ensure!(
                    status.success(),
                    "{} lacks {architecture}",
                    library.display()
                );
            }
        }
    }
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
            "crates/cutout-core/src/lib.rs",
        ] {
            fs::write(root.join(file), "original\n").unwrap();
        }

        let before = source_fingerprint(&root).unwrap();
        fs::write(root.join("crates/cutout-core/src/lib.rs"), "changed\n").unwrap();
        let after = source_fingerprint(&root).unwrap();
        fs::remove_dir_all(root).unwrap();

        assert_ne!(before, after);
    }
}
