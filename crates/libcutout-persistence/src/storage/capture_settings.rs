//! Reads worksheet evidence from the existing ordered capture stream.

use super::capture_submission::SourceRecord;
use super::{
    CaptureSubmissionError, CaptureSubmissionSource, LiveCaptureId, LiveCaptureSnapshot,
    QueryLimit, RideDatabase, StorageError,
};
use cutout_core::{
    CaptureSettingsObservation, MAX_CAPTURE_SETTINGS_OBSERVATIONS, PevcapEncoding, PevcapEvent,
    PevcapReader, PevcapRecord,
};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Cursor, Read};

/// A bounded, pinned stream owned independently of the active capture writer.
pub(crate) struct CaptureTransportReadPrefix {
    source: Box<dyn Read + Send>,
}

impl CaptureTransportReadPrefix {
    pub(crate) fn file(file: std::fs::File, bytes: u64) -> Self {
        Self {
            source: Box::new(file.take(bytes)),
        }
    }

    pub(crate) fn live(database: RideDatabase, id: LiveCaptureId) -> Result<Self, StorageError> {
        Ok(Self {
            source: Box::new(LiveCaptureReader::new(database, id)?),
        })
    }

    pub(crate) fn visit(self, mut visit: impl FnMut(PevcapRecord)) -> Result<(), String> {
        visit_events(self.source, PevcapEncoding::Jsonl, |event| {
            if let PevcapEvent::Record(record) = event {
                visit(record);
            }
            Ok(())
        })
        .map_err(|error| error.to_string())
    }
}

impl RideDatabase {
    /// Reads successive snapshots without changing capture or prepared Send bytes.
    ///
    /// Reads active and finalized captures in bounded event pages. Call after a writer
    /// flush when the caller needs an acceptance result rather than an eventual view.
    ///
    /// # Errors
    /// Returns a missing capture, malformed stored observation, or storage-bound error.
    pub fn live_capture_settings_observations(
        &self,
        id: LiveCaptureId,
    ) -> Result<Vec<CaptureSettingsObservation>, StorageError> {
        let mut observations = Vec::new();
        visit_live_events(self.clone(), id, |event| {
            if let PevcapEvent::SettingsObservation(observation) = event {
                push(&mut observations, observation)?;
            }
            Ok(())
        })?;
        Ok(observations)
    }

    /// Reads a completed recording's observations, including file-backed recordings
    /// whose original and managed files no longer exist.
    ///
    /// The original canonical SQLite chunks are streamed and hash-verified without
    /// exporting a temporary artifact or preparing a Send operation.
    ///
    /// # Errors
    /// Returns a missing source, invalid capture, integrity, or database error.
    pub fn capture_settings_observations(
        &self,
        source: CaptureSubmissionSource,
    ) -> Result<Vec<CaptureSettingsObservation>, CaptureSubmissionError> {
        let mut observations = Vec::new();
        self.visit_capture_events(source, |event| {
            if let PevcapEvent::SettingsObservation(observation) = event {
                push(&mut observations, observation)?;
            }
            Ok(())
        })?;
        Ok(observations)
    }

    /// Visits original transport records for passive decoder comparison without
    /// exporting a file or accumulating a complete recording in memory.
    ///
    /// Active database reads retain the event prefix visible at the first page.
    ///
    /// # Errors
    /// Returns a missing source, invalid capture, integrity, or database error.
    pub fn visit_capture_transport_records(
        &self,
        source: CaptureSubmissionSource,
        mut visit: impl FnMut(PevcapRecord),
    ) -> Result<(), CaptureSubmissionError> {
        self.visit_capture_events(source, |event| {
            if let PevcapEvent::Record(record) = event {
                visit(record);
            }
            Ok(())
        })
    }

    fn visit_capture_events(
        &self,
        source: CaptureSubmissionSource,
        visit: impl FnMut(PevcapEvent) -> Result<(), StorageError>,
    ) -> Result<(), CaptureSubmissionError> {
        match self.submission_source(source)? {
            SourceRecord::Live(entry) => {
                visit_live_events(self.clone(), entry.id, visit).map_err(Into::into)
            }
            SourceRecord::File(receipt) => {
                let source = StoredCaptureReader {
                    database: self.clone(),
                    digest: receipt.artifact_digest,
                    sequence: 0,
                    chunk: Cursor::new(Vec::new()),
                    hash: Sha256::new(),
                    bytes: 0,
                    complete: false,
                };
                let mut source = BufReader::new(source);
                let encoding = if source
                    .fill_buf()
                    .map_err(StorageError::from)?
                    .starts_with(&cutout_core::PEVCAP_MAGIC)
                {
                    PevcapEncoding::Binary
                } else {
                    PevcapEncoding::Jsonl
                };
                visit_events(source, encoding, visit).map_err(Into::into)
            }
        }
    }
}

fn push(
    values: &mut Vec<CaptureSettingsObservation>,
    observation: CaptureSettingsObservation,
) -> Result<(), StorageError> {
    if values.len() >= MAX_CAPTURE_SETTINGS_OBSERVATIONS {
        return Err(StorageError::LiveCaptureInputInvalid(
            "settings observation count",
        ));
    }
    values.push(observation);
    Ok(())
}

fn visit_live_events(
    database: RideDatabase,
    id: LiveCaptureId,
    mut visit: impl FnMut(PevcapEvent) -> Result<(), StorageError>,
) -> Result<(), StorageError> {
    let source = LiveCaptureReader::new(database, id)?;
    let capture_id = id.to_string();
    visit_events(source, PevcapEncoding::Jsonl, |event| {
        if let PevcapEvent::SettingsObservation(observation) = &event
            && observation.capture_id != capture_id
        {
            return Err(StorageError::LiveCaptureInputInvalid(
                "settings observation capture identity",
            ));
        }
        visit(event)
    })
}

fn visit_events(
    source: impl Read,
    encoding: PevcapEncoding,
    mut visit: impl FnMut(PevcapEvent) -> Result<(), StorageError>,
) -> Result<(), StorageError> {
    let mut source = BufReader::new(source);
    {
        let mut reader = PevcapReader::new(&mut source, encoding)
            .map_err(|error| StorageError::PevcapImport(error.to_string()))?;
        while let Some(event) = reader
            .next_event()
            .map_err(|error| StorageError::PevcapImport(error.to_string()))?
        {
            visit(event)?;
        }
    }
    // EOF also settles the original-byte reader's digest, including binary input.
    let trailing = io::copy(&mut source, &mut io::sink())?;
    if trailing != 0 {
        return Err(StorageError::PevcapImport(
            "capture has trailing bytes".into(),
        ));
    }
    Ok(())
}

struct LiveCaptureReader {
    database: RideDatabase,
    id: LiveCaptureId,
    next_sequence: u64,
    after_sequence: Option<u64>,
    lines: VecDeque<Vec<u8>>,
    current: Cursor<Vec<u8>>,
    complete: bool,
}

impl LiveCaptureReader {
    fn new(database: RideDatabase, id: LiveCaptureId) -> Result<Self, StorageError> {
        let page = database.live_capture_page(id, None, QueryLimit::new(32)?)?;
        let mut lines = VecDeque::new();
        let mut header = page.header_json.clone();
        header.push(b'\n');
        lines.push_back(header);
        let mut reader = Self {
            database,
            id,
            next_sequence: page.next_sequence,
            after_sequence: None,
            lines,
            current: Cursor::new(Vec::new()),
            complete: false,
        };
        reader.include_page(page);
        Ok(reader)
    }

    fn include_page(&mut self, page: LiveCaptureSnapshot) {
        let next_sequence = self.next_sequence;
        for event in page
            .events
            .into_iter()
            .take_while(|event| event.sequence < next_sequence)
        {
            self.after_sequence = Some(event.sequence);
            let mut line = event.payload;
            line.push(b'\n');
            self.lines.push_back(line);
        }
        self.complete = self
            .after_sequence
            .and_then(|sequence| sequence.checked_add(1))
            .is_none_or(|next| next >= self.next_sequence);
    }
}

impl Read for LiveCaptureReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        loop {
            let count = self.current.read(output)?;
            if count != 0 {
                return Ok(count);
            }
            if let Some(line) = self.lines.pop_front() {
                self.current = Cursor::new(line);
            } else if self.complete {
                return Ok(0);
            } else {
                let page = self
                    .database
                    .live_capture_page(
                        self.id,
                        self.after_sequence,
                        QueryLimit::new(32).map_err(io::Error::other)?,
                    )
                    .map_err(io::Error::other)?;
                if page.events.is_empty() {
                    return Err(io::Error::other("capture event prefix changed"));
                }
                self.include_page(page);
            }
        }
    }
}

struct StoredCaptureReader {
    database: RideDatabase,
    digest: String,
    sequence: u64,
    chunk: Cursor<Vec<u8>>,
    hash: Sha256,
    bytes: u64,
    complete: bool,
}

impl Read for StoredCaptureReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() || self.complete {
            return Ok(0);
        }
        loop {
            let count = self.chunk.read(output)?;
            if count != 0 {
                self.bytes = self
                    .bytes
                    .checked_add(count as u64)
                    .filter(|count| *count <= super::LIVE_CAPTURE_TOTAL_LIMIT_BYTES)
                    .ok_or_else(|| io::Error::other("capture bytes exceed storage bound"))?;
                self.hash.update(&output[..count]);
                return Ok(count);
            }
            if let Some(chunk) = self
                .database
                .pevcap_capture_chunk(&self.digest, self.sequence)
                .map_err(|_| io::Error::other("capture chunk is unavailable"))?
            {
                self.sequence = self
                    .sequence
                    .checked_add(1)
                    .ok_or_else(|| io::Error::other("capture chunk sequence overflow"))?;
                self.chunk = Cursor::new(chunk);
            } else {
                if self.bytes == 0 || hex::encode(self.hash.clone().finalize()) != self.digest {
                    return Err(io::Error::other("capture byte digest changed"));
                }
                self.complete = true;
                return Ok(0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutout_core::{PevcapCapture, PevcapHeader, WallClockUnixTimestamp};

    fn binary_capture() -> Vec<u8> {
        let header = PevcapHeader::new(
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "wheel-a",
            None,
            &[],
            &[],
            None,
            None,
            "test",
            [0; 32],
            &[],
        )
        .unwrap();
        PevcapCapture::new(header, vec![])
            .encode(PevcapEncoding::Binary)
            .unwrap()
    }

    fn read_stored_capture(database: &RideDatabase, digest: &str) -> Result<(), StorageError> {
        let source = StoredCaptureReader {
            database: database.clone(),
            digest: digest.into(),
            sequence: 0,
            chunk: Cursor::new(Vec::new()),
            hash: Sha256::new(),
            bytes: 0,
            complete: false,
        };
        visit_events(source, PevcapEncoding::Binary, |_| Ok(()))
    }

    #[test]
    fn stored_binary_reader_rejects_truncation_trailing_bytes_and_valid_digest_drift() {
        let original = binary_capture();
        let mut truncated = original.clone();
        truncated.pop();
        let mut trailing = original.clone();
        trailing.push(0);
        let mut changed_header = original.clone();
        let offset = changed_header
            .windows(7)
            .position(|window| window == b"wheel-a")
            .unwrap();
        changed_header[offset + 6] = b'b';
        assert!(
            PevcapCapture::decode(&changed_header, PevcapEncoding::Binary).is_ok(),
            "digest test must remain structurally valid"
        );
        for changed in [truncated, trailing, changed_header] {
            let directory = tempfile::tempdir().unwrap();
            let database_path = directory.path().join("ride.sqlite");
            let database = RideDatabase::open(&database_path).unwrap();
            let path = directory.path().join("capture.pevcap");
            std::fs::write(&path, &original).unwrap();
            let preview = database
                .preflight_pevcap(&path, PevcapEncoding::Binary)
                .unwrap();
            let receipt = database
                .confirm_pevcap_import(&preview, 1_700_000_001_000)
                .unwrap();
            read_stored_capture(&database, &receipt.artifact_digest).unwrap();
            let connection = rusqlite::Connection::open(database_path).unwrap();
            assert_eq!(connection.execute("UPDATE pevcap_capture_chunks SET payload=?1 WHERE artifact_digest=?2 AND sequence=0", rusqlite::params![changed, receipt.artifact_digest]).unwrap(), 1);
            assert!(read_stored_capture(&database, &receipt.artifact_digest).is_err());
            drop(connection);
            database.shutdown().unwrap();
        }
    }
}
