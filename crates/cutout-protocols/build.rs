use std::{
    collections::BTreeSet,
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
struct RegisteredModelSource {
    module: String,
    definition_const: String,
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("cargo manifest dir"));
    emit_replay_decoder_fingerprint(&manifest_dir);
    let source_path = manifest_dir.join("registry/models.json");
    println!("cargo:rerun-if-changed={}", source_path.display());

    let sources = load_sources(&source_path);
    validate_sources(&sources, &source_path);
    let generated = render_sources(&sources);

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("cargo out dir"));
    let output_path = out_dir.join("registry_models.rs");
    fs::write(&output_path, generated).expect("write generated registry models");
}

/// Fingerprints the interpreter source, rather than a Git ref or the original capture library.
///
/// Scope: workspace Cargo.toml and Cargo.lock; core/protocol Cargo.toml files; this build.rs;
/// all Rust files recursively under both crates' src directories; and all files recursively
/// under the protocol registry directory. Generated output, host paths, environment values,
/// timestamps, enabled features, and compiler identity are not part of this source fingerprint.
///
/// Packaged crates without this complete workspace scope leave the fingerprint unavailable;
/// they never substitute a partial digest. The candidate workspace must contain this exact
/// protocol crate, so unrelated parent directories cannot supply sibling source inputs.
///
/// SHA256 input starts with the versioned domain below, followed by inputs sorted by their
/// slash-separated workspace-relative names. Each input is framed as an eight-byte big-endian
/// name length, name bytes, an eight-byte big-endian content length, and exact file bytes.
fn emit_replay_decoder_fingerprint(manifest_dir: &Path) {
    let Some(workspace) = manifest_dir.parent().and_then(Path::parent) else {
        return;
    };
    let Ok(actual_crate) = fs::canonicalize(manifest_dir) else {
        return;
    };
    if fs::canonicalize(workspace.join("crates/cutout-protocols")).ok() != Some(actual_crate) {
        return;
    }
    let mut inputs = BTreeSet::from([
        "Cargo.toml".to_owned(),
        "Cargo.lock".to_owned(),
        "crates/cutout-core/Cargo.toml".to_owned(),
        "crates/cutout-protocols/Cargo.toml".to_owned(),
        "crates/cutout-protocols/build.rs".to_owned(),
        "crates/cutout-core/src/lib.rs".to_owned(),
        "crates/cutout-protocols/src/lib.rs".to_owned(),
        "crates/cutout-protocols/registry/models.json".to_owned(),
    ]);
    let directories = [
        ("crates/cutout-core/src", true),
        ("crates/cutout-protocols/src", true),
        ("crates/cutout-protocols/registry", false),
    ];
    if inputs
        .iter()
        .any(|relative| !workspace.join(relative).is_file())
        || directories
            .iter()
            .any(|(directory, _)| !workspace.join(directory).is_dir())
    {
        return;
    }
    for (directory, rust_only) in directories {
        collect_replay_decoder_inputs(workspace, directory, rust_only, &mut inputs);
    }

    let mut digest = Sha256::new();
    digest.update(b"cutout-replay-decoder-source-v1\0");
    for relative in inputs {
        // Cargo resolves these paths relative to this crate, independently of the host checkout.
        println!("cargo:rerun-if-changed=../../{relative}");
        let bytes = fs::read(workspace.join(&relative))
            .unwrap_or_else(|error| panic!("read decoder fingerprint input {relative}: {error}"));
        digest.update(
            u64::try_from(relative.len())
                .expect("fingerprint input name length fits u64")
                .to_be_bytes(),
        );
        digest.update(relative.as_bytes());
        digest.update(
            u64::try_from(bytes.len())
                .expect("fingerprint input content length fits u64")
                .to_be_bytes(),
        );
        digest.update(&bytes);
    }
    println!(
        "cargo:rustc-env=CUTOUT_REPLAY_DECODER_SOURCE_SHA256={:x}",
        digest.finalize()
    );
}

fn collect_replay_decoder_inputs(
    workspace: &Path,
    directory: &str,
    rust_only: bool,
    inputs: &mut BTreeSet<String>,
) {
    // Watching each directory detects additions/removals, including new nested source trees.
    println!("cargo:rerun-if-changed=../../{directory}");
    let entries = fs::read_dir(workspace.join(directory))
        .unwrap_or_else(|error| panic!("read decoder fingerprint directory {directory}: {error}"));
    for entry in entries {
        let entry = entry.expect("read decoder fingerprint directory entry");
        let name = entry.file_name();
        let name = name
            .to_str()
            .expect("decoder fingerprint input names must be UTF-8");
        let relative = format!("{directory}/{name}");
        if entry
            .file_type()
            .expect("read decoder fingerprint input type")
            .is_dir()
        {
            collect_replay_decoder_inputs(workspace, &relative, rust_only, inputs);
        } else if !rust_only
            || entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "rs")
        {
            inputs.insert(relative);
        }
    }
}

fn load_sources(path: &Path) -> Vec<RegisteredModelSource> {
    let contents = fs::read_to_string(path).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", path.display());
    });
    serde_json::from_str(&contents).unwrap_or_else(|error| {
        panic!("failed to parse {} as JSON: {error}", path.display());
    })
}

fn validate_sources(sources: &[RegisteredModelSource], path: &Path) {
    assert!(
        !sources.is_empty(),
        "{} must define at least one registered model",
        path.display()
    );

    let mut seen = BTreeSet::new();
    for source in sources {
        assert!(
            !source.module.trim().is_empty(),
            "{} contains an empty module name",
            path.display()
        );
        assert!(
            !source.definition_const.trim().is_empty(),
            "{} contains an empty definition const name",
            path.display()
        );
        let key = (&source.module, &source.definition_const);
        assert!(
            seen.insert(key),
            "{} contains duplicate module/definition entries for {}::{}",
            path.display(),
            source.module,
            source.definition_const
        );
    }
}

fn render_sources(sources: &[RegisteredModelSource]) -> String {
    let mut output = String::from("// @generated by build.rs; do not edit by hand.\n\n");

    write!(
        output,
        "/// Structured source data for statically registered models.\n\
         pub const REGISTERED_MODEL_DEFINITIONS: [RegisteredModelDefinition; {}] = [\n",
        sources.len()
    )
    .expect("write generated registry definitions");
    for source in sources {
        writeln!(
            output,
            "    {}::{},\n",
            source.module, source.definition_const
        )
        .expect("write generated registry definition source");
    }
    output.push_str("];\n\n");

    write!(
        output,
        "/// Registry entries exposed to identification and validation.\n\
         pub const MODEL_REGISTRY: [&ModelRegistryEntry; {}] = [\n",
        sources.len()
    )
    .expect("write generated model registry");
    for (index, _) in sources.iter().enumerate() {
        writeln!(
            output,
            "    REGISTERED_MODEL_DEFINITIONS[{index}].registry,"
        )
        .expect("write generated model registry entry");
    }
    output.push_str("];\n\n");

    write!(
        output,
        "/// Catalog entries exposed to model resolution and host surfaces.\n\
         pub const MODEL_CATALOG: [ModelCatalogEntry; {}] = [\n",
        sources.len()
    )
    .expect("write generated model catalog");
    for (index, _) in sources.iter().enumerate() {
        writeln!(
            output,
            "    REGISTERED_MODEL_DEFINITIONS[{index}].catalog_entry(),"
        )
        .expect("write generated model catalog entry");
    }
    output.push_str("];\n\n");

    write!(
        output,
        "/// Session constructors keyed by the generated model catalog.\n\
         pub const SESSION_REGISTRATIONS: [SessionRegistration; {}] = [\n",
        sources.len()
    )
    .expect("write generated session registrations");
    for (index, _) in sources.iter().enumerate() {
        writeln!(
            output,
            "    REGISTERED_MODEL_DEFINITIONS[{index}].session_registration(),"
        )
        .expect("write generated session registration entry");
    }
    output.push_str("];\n");

    output
}
