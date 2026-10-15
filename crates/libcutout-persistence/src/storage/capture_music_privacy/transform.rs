//! Bounded PEVCAP transformations retaining opaque non-music JSON values.
use super::StorageError;
use cutout_core::{PEVCAP_MAGIC, PevcapEncoding, PevcapFormatVersion};
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use std::io::{BufRead, BufReader, Read, Write};

const JSON_LIMIT: u64 = 1_048_576;

pub(super) fn invalid() -> StorageError {
    StorageError::LiveCaptureInputInvalid(
        "capture metadata cleanup requires a valid bounded capture",
    )
}

struct Object(Vec<(String, Box<RawValue>)>);
impl<'de> Deserialize<'de> for Object {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Entries;
        impl<'de> Visitor<'de> for Entries {
            type Value = Object;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a JSON object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Object, M::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(Object(entries))
            }
        }
        deserializer.deserialize_map(Entries)
    }
}

fn object(bytes: &[u8]) -> Result<Object, StorageError> {
    serde_json::from_slice(bytes).map_err(|_| invalid())
}
fn serialize(value: &Object) -> Result<Vec<u8>, StorageError> {
    let mut output = Vec::new();
    output.push(b'{');
    for (index, (key, value)) in value.0.iter().enumerate() {
        if index != 0 {
            output.push(b',');
        }
        serde_json::to_writer(&mut output, key).map_err(|_| invalid())?;
        output.push(b':');
        output.extend_from_slice(value.get().as_bytes());
    }
    output.push(b'}');
    Ok(output)
}
fn strip_record(bytes: &[u8]) -> Result<(Vec<u8>, bool), StorageError> {
    let mut value = object(bytes)?;
    let changed = value.0.iter().any(|(key, _)| key == "music");
    if !changed {
        return Ok((bytes.to_vec(), false));
    }
    value.0.retain(|(key, _)| key != "music");
    Ok((serialize(&value)?, true))
}

/// Returns None for standalone music; otherwise retains original bytes unless changed.
pub(super) fn line(bytes: &[u8]) -> Result<(Option<Vec<u8>>, bool), StorageError> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok((Some(bytes.to_vec()), false));
    }
    let mut value = object(bytes)?;
    let kinds: Vec<String> = value
        .0
        .iter()
        .filter(|(key, _)| key == "kind")
        .map(|(_, raw)| serde_json::from_str(raw.get()).map_err(|_| invalid()))
        .collect::<Result<_, _>>()?;
    if kinds.iter().any(|kind| kind == "music") {
        return Ok((None, true));
    }
    if !kinds.iter().any(|kind| kind == "record") {
        return Ok((Some(bytes.to_vec()), false));
    }
    let mut changed = false;
    for (key, raw) in &mut value.0 {
        if key == "record" {
            let (record, removed) = strip_record(raw.get().as_bytes())?;
            if removed {
                *raw = RawValue::from_string(String::from_utf8(record).map_err(|_| invalid())?)
                    .map_err(|_| invalid())?;
                changed = true;
            }
        }
    }
    if !changed {
        return Ok((Some(bytes.to_vec()), false));
    }
    let mut output = serialize(&value)?;
    if bytes.ends_with(b"\n") {
        output.push(b'\n');
    }
    Ok((Some(output), true))
}

fn read_u32(source: &mut impl Read) -> Result<u32, StorageError> {
    let mut bytes = [0; 4];
    source.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}
fn read_json(source: &mut impl Read) -> Result<Vec<u8>, StorageError> {
    let count = read_u32(source)?;
    if count == 0 || u64::from(count) > JSON_LIMIT {
        return Err(invalid());
    }
    let mut bytes = vec![0; count as usize];
    source.read_exact(&mut bytes)?;
    Ok(bytes)
}
fn write_json(output: &mut impl Write, bytes: &[u8]) -> Result<(), StorageError> {
    output.write_all(
        &u32::try_from(bytes.len())
            .map_err(|_| invalid())?
            .to_le_bytes(),
    )?;
    output.write_all(bytes)?;
    Ok(())
}

pub(crate) fn remove_music(
    source: &mut impl Read,
    output: &mut impl Write,
    encoding: PevcapEncoding,
) -> Result<bool, StorageError> {
    remove_with_provenance(source, output, encoding, None)
}
fn mark(bytes: &[u8], digest: &str) -> Result<Vec<u8>, StorageError> {
    let mut value = object(bytes)?;
    value
        .0
        .retain(|(key, _)| key != "cutout_music_removed_source_sha256");
    value.0.push((
        "cutout_music_removed_source_sha256".to_owned(),
        RawValue::from_string(format!("\"{digest}\"")).map_err(|_| invalid())?,
    ));
    serialize(&value)
}
pub(super) fn remove_with_provenance(
    source: &mut impl Read,
    output: &mut impl Write,
    encoding: PevcapEncoding,
    provenance: Option<&str>,
) -> Result<bool, StorageError> {
    match encoding {
        PevcapEncoding::Jsonl => {
            let mut source = BufReader::new(source);
            let mut changed = false;
            loop {
                let mut bytes = Vec::new();
                let count = source
                    .by_ref()
                    .take(JSON_LIMIT + 1)
                    .read_until(b'\n', &mut bytes)?;
                if count == 0 {
                    break;
                }
                if count as u64 > JSON_LIMIT {
                    return Err(invalid());
                }
                let (retained, removed) = line(&bytes)?;
                changed |= removed;
                if let Some(bytes) = retained {
                    if let Some(digest) = provenance.filter(|_| {
                        object(&bytes).is_ok_and(|o| {
                            o.0.iter().any(|(key, value)| {
                                key == "kind"
                                    && serde_json::from_str::<String>(value.get())
                                        .is_ok_and(|value| value == "header")
                            })
                        })
                    }) {
                        output.write_all(&mark(&bytes, digest)?)?;
                        output.write_all(b"\n")?;
                    } else {
                        output.write_all(&bytes)?;
                    }
                }
            }
            Ok(changed)
        }
        PevcapEncoding::Binary => {
            let mut prefix = [0; 12];
            source.read_exact(&mut prefix)?;
            if prefix[..8] != PEVCAP_MAGIC {
                return Err(invalid());
            }
            let version = PevcapFormatVersion {
                major: u16::from_le_bytes([prefix[8], prefix[9]]),
                minor: u16::from_le_bytes([prefix[10], prefix[11]]),
            };
            if !version.is_supported() {
                return Err(invalid());
            }
            output.write_all(&prefix)?;
            let header = read_json(source)?;
            write_json(
                output,
                &match provenance {
                    Some(digest) => mark(&header, digest)?,
                    None => header,
                },
            )?;
            let count = read_u32(source)?;
            output.write_all(&count.to_le_bytes())?;
            let mut changed = false;
            for _ in 0..count {
                let (bytes, removed) = strip_record(&read_json(source)?)?;
                changed |= removed;
                write_json(output, &bytes)?;
            }
            if version.supports_locations() {
                let count = read_u32(source)?;
                output.write_all(&count.to_le_bytes())?;
                for _ in 0..count {
                    write_json(output, &read_json(source)?)?;
                }
            }
            if version.supports_music() {
                let count = read_u32(source)?;
                changed |= count != 0;
                output.write_all(&0_u32.to_le_bytes())?;
                for _ in 0..count {
                    let _ = read_json(source)?;
                }
            }
            let mut trailing = [0];
            if source.read(&mut trailing)? != 0 {
                return Err(invalid());
            }
            Ok(changed)
        }
    }
}
