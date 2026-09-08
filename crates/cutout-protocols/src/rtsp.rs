//! Retina-backed RTSP preview transport for camera sources.

use std::{
    fs::File,
    io::{self, BufWriter, Write},
    net::Ipv4Addr,
    path::Path,
};

use crate::NovatekHttpOrigin;
use futures_util::StreamExt;
use retina::{
    client,
    codec::{CodecItem, ParametersRef},
};
use thiserror::Error;
use url::Url;

/// Maximum encoded H.264 access-unit size accepted from an RTSP camera.
const RETINA_MAX_VIDEO_FRAME_BYTES: usize = 8 * 1024 * 1024;
const RETINA_MAX_VIDEO_CONFIGURATION_BYTES: usize = 64 * 1024;

/// Failure while creating or consuming a Retina RTSP preview session.
#[derive(Debug, Error)]
pub enum RetinaRtspError {
    /// The supplied URI was not an RTSP URL.
    #[error("invalid RTSP URI")]
    InvalidUri,
    /// The RTSP endpoint is not a validated local IPv4 camera origin.
    #[error("RTSP endpoint is not local")]
    NonLocalUri,
    /// The RTSP endpoint differs from the explicitly selected camera origin.
    #[error("RTSP endpoint does not match the camera origin")]
    OriginMismatch,
    /// The server's SDP did not advertise an H.264 video stream.
    #[error("RTSP session has no H.264 video stream")]
    UnsupportedVideoCodec,
    /// Retina could not establish or negotiate the session.
    #[error("RTSP session error: {0}")]
    Session(String),
    /// A decoded access unit exceeded the mobile preview memory budget.
    #[error("RTSP video frame exceeds {max} bytes")]
    VideoFrameTooLarge {
        /// Maximum accepted encoded frame size.
        max: usize,
    },
}

/// One encoded video access unit emitted by a Retina preview session.
#[derive(Debug, Eq, PartialEq)]
pub struct RetinaVideoFrame {
    /// Encoded frame bytes in Retina's codec-specific format.
    pub data: Vec<u8>,
    /// Number of lost RTP packets before this frame.
    pub loss: u16,
    /// Whether this frame is a random-access point.
    pub is_random_access_point: bool,
    /// Presentation timestamp in the stream's clock units.
    pub timestamp: i64,
    /// Clock rate associated with [`Self::timestamp`], in Hz.
    pub clock_rate_hz: u32,
}

/// Codec configuration advertised by the RTSP video stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetinaVideoConfiguration {
    /// RFC 6381 codec identifier, for example `avc1.4D401E`.
    pub codec: String,
    /// Coded width in pixels.
    pub width: u32,
    /// Coded height in pixels.
    pub height: u32,
    /// Codec-specific decoder configuration (H.264 `avcC` bytes).
    pub extra_data: Vec<u8>,
}

impl RetinaVideoFrame {
    /// Creates one encoded video access unit with its stream timing.
    #[must_use]
    pub const fn new(
        data: Vec<u8>,
        loss: u16,
        is_random_access_point: bool,
        timestamp: i64,
        clock_rate_hz: u32,
    ) -> Self {
        Self {
            data,
            loss,
            is_random_access_point,
            timestamp,
            clock_rate_hz,
        }
    }
}

/// A file sink for Retina's H.264 access units.
///
/// The output is an Annex-B H.264 elementary stream. It is intentionally not
/// an MP4/MOV container: callers can hand the resulting file to a decoder or
/// muxer that understands H.264 elementary streams, while the transport layer
/// remains independent of platform media frameworks.
pub struct RetinaH264FileSink {
    writer: BufWriter<File>,
    frame_count: u64,
}

impl std::fmt::Debug for RetinaH264FileSink {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RetinaH264FileSink")
            .field("frame_count", &self.frame_count)
            .finish_non_exhaustive()
    }
}

impl RetinaH264FileSink {
    /// Creates or truncates an Annex-B H.264 output file.
    ///
    /// # Errors
    ///
    /// Returns the filesystem error if the destination cannot be created.
    pub fn create(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            writer: BufWriter::new(File::create(path)?),
            frame_count: 0,
        })
    }

    /// Writes one Retina H.264 access unit to the output file.
    ///
    /// Retina emits each NAL unit with a four-byte big-endian length prefix;
    /// this sink replaces those prefixes with Annex-B start codes.
    ///
    /// # Errors
    ///
    /// Returns [`io::ErrorKind::InvalidData`] for malformed length-prefixed
    /// data, or the underlying filesystem error when writing.
    pub fn write_frame(&mut self, frame: &RetinaVideoFrame) -> io::Result<()> {
        validate_length_prefixed_access_unit(&frame.data)?;

        let mut cursor = 0;
        while cursor < frame.data.len() {
            let nal_length = read_nal_length(&frame.data, cursor)?;
            cursor += 4;
            let nal_end = cursor
                .checked_add(nal_length)
                .ok_or_else(|| invalid_data("H.264 NAL length overflows access unit"))?;
            self.writer.write_all(&[0, 0, 0, 1])?;
            self.writer.write_all(
                frame
                    .data
                    .get(cursor..nal_end)
                    .ok_or_else(|| invalid_data("H.264 NAL length exceeds access unit"))?,
            )?;
            cursor = nal_end;
        }
        self.frame_count += 1;
        Ok(())
    }

    /// Returns the number of access units accepted by this sink.
    #[must_use]
    pub const fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Flushes buffered bytes and finishes the file.
    ///
    /// # Errors
    ///
    /// Returns the underlying filesystem error if buffered bytes cannot be
    /// flushed.
    pub fn finish(mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

fn validate_length_prefixed_access_unit(data: &[u8]) -> io::Result<()> {
    if data.is_empty() {
        return Err(invalid_data("H.264 access unit is empty"));
    }

    let mut cursor = 0;
    while cursor < data.len() {
        let nal_length = read_nal_length(data, cursor)?;
        cursor += 4;
        if nal_length == 0 || nal_length > data.len() - cursor {
            return Err(invalid_data("H.264 NAL length exceeds access unit"));
        }
        cursor += nal_length;
    }
    Ok(())
}

fn read_nal_length(data: &[u8], cursor: usize) -> io::Result<usize> {
    let prefix_end = cursor
        .checked_add(4)
        .ok_or_else(|| invalid_data("H.264 NAL length prefix overflows access unit"))?;
    let prefix = data
        .get(cursor..prefix_end)
        .ok_or_else(|| invalid_data("H.264 NAL length prefix is truncated"))?;
    let prefix: [u8; 4] = prefix
        .try_into()
        .map_err(|_| invalid_data("H.264 NAL length prefix is truncated"))?;
    Ok(u32::from_be_bytes(prefix) as usize)
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// A running Retina RTSP session restricted to the first advertised video stream.
///
/// The session owns the transport and ends when dropped. It deliberately does
/// not impose an application timeout: callers stop it by dropping the session
/// or cancelling the task that is awaiting [`Self::next_video_frame`].
pub struct RetinaRtspPreviewSession {
    demuxed: client::Demuxed,
    video_stream_id: usize,
    video_configuration: Option<RetinaVideoConfiguration>,
}

impl std::fmt::Debug for RetinaRtspPreviewSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RetinaRtspPreviewSession")
            .field("video_stream_id", &self.video_stream_id)
            .finish_non_exhaustive()
    }
}

impl RetinaRtspPreviewSession {
    /// Connects to an RTSP URI, negotiates TCP interleaving, and starts playback.
    ///
    /// # Errors
    ///
    /// Returns an error if the URI is invalid, no video stream is advertised,
    /// or Retina cannot complete DESCRIBE/SETUP/PLAY.
    pub async fn connect(uri: &str) -> Result<Self, RetinaRtspError> {
        Self::connect_inner(uri, None).await
    }

    /// Connects only when the RTSP host matches the selected camera origin.
    ///
    /// The live-view URI is camera-reported input. Binding it to the origin
    /// already selected by the caller prevents a camera response from
    /// redirecting the preview to another private-network host.
    ///
    /// # Errors
    ///
    /// Returns an error when the URI is malformed, non-local, or names a
    /// different address than `expected_address`, or when the RTSP handshake
    /// fails.
    pub async fn connect_for_origin(
        uri: &str,
        expected_address: Ipv4Addr,
    ) -> Result<Self, RetinaRtspError> {
        Self::connect_inner(uri, Some(expected_address)).await
    }

    async fn connect_inner(
        uri: &str,
        expected_address: Option<Ipv4Addr>,
    ) -> Result<Self, RetinaRtspError> {
        let url = Url::parse(uri).map_err(|_| RetinaRtspError::InvalidUri)?;
        if url.scheme() != "rtsp" || url.host_str().is_none() {
            return Err(RetinaRtspError::InvalidUri);
        }
        let host = url
            .host_str()
            .and_then(|host| host.parse::<Ipv4Addr>().ok())
            .ok_or(RetinaRtspError::NonLocalUri)?;
        if expected_address.is_some_and(|expected| expected != host) {
            return Err(RetinaRtspError::OriginMismatch);
        }
        NovatekHttpOrigin::new(host, url.port().unwrap_or(554))
            .map_err(|_| RetinaRtspError::NonLocalUri)?;

        let mut described = client::Session::describe(url, client::SessionOptions::default())
            .await
            .map_err(|error| RetinaRtspError::Session(error.to_string()))?;
        let video_stream_id = described
            .streams()
            .iter()
            .position(|stream| stream.media() == "video" && stream.encoding_name() == "h264")
            .ok_or(RetinaRtspError::UnsupportedVideoCodec)?;
        described
            .setup(
                video_stream_id,
                client::SetupOptions::default().transport(client::Transport::default()),
            )
            .await
            .map_err(|error| RetinaRtspError::Session(error.to_string()))?;
        let playing = described
            .play(client::PlayOptions::default())
            .await
            .map_err(|error| RetinaRtspError::Session(error.to_string()))?;
        let demuxed = playing
            .demuxed()
            .map_err(|error| RetinaRtspError::Session(error.to_string()))?;

        let video_configuration =
            video_configuration_for_stream(demuxed.streams().get(video_stream_id));

        Ok(Self {
            demuxed,
            video_stream_id,
            video_configuration,
        })
    }

    /// Returns codec configuration advertised by SDP, when available and
    /// within the fixed boundary budget.
    #[must_use]
    pub fn video_configuration(&self) -> Option<&RetinaVideoConfiguration> {
        self.video_configuration.as_ref()
    }

    /// Waits for the next encoded video access unit.
    ///
    /// # Errors
    ///
    /// Returns a Retina error when the transport or depacketizer fails.
    pub async fn next_video_frame(&mut self) -> Result<Option<RetinaVideoFrame>, RetinaRtspError> {
        while let Some(item) = self.demuxed.next().await {
            match item.map_err(|error| RetinaRtspError::Session(error.to_string()))? {
                CodecItem::VideoFrame(frame) if frame.stream_id() == self.video_stream_id => {
                    if frame.has_new_parameters() {
                        self.video_configuration = video_configuration_for_stream(
                            self.demuxed.streams().get(self.video_stream_id),
                        );
                    }
                    let loss = frame.loss();
                    let is_random_access_point = frame.is_random_access_point();
                    let timestamp = frame.timestamp();
                    let data = frame.into_data();
                    ensure_video_frame_size(data.len())?;
                    return Ok(Some(RetinaVideoFrame::new(
                        data,
                        loss,
                        is_random_access_point,
                        timestamp.timestamp(),
                        timestamp.clock_rate().get(),
                    )));
                }
                _ => {}
            }
        }
        Ok(None)
    }
}

fn video_configuration_for_stream(
    stream: Option<&client::Stream>,
) -> Option<RetinaVideoConfiguration> {
    stream
        .and_then(client::Stream::parameters)
        .and_then(|parameters| match parameters {
            ParametersRef::Video(parameters)
                if parameters.extra_data().len() <= RETINA_MAX_VIDEO_CONFIGURATION_BYTES =>
            {
                Some(RetinaVideoConfiguration {
                    codec: parameters.rfc6381_codec().to_owned(),
                    width: parameters.pixel_dimensions().0,
                    height: parameters.pixel_dimensions().1,
                    extra_data: parameters.extra_data().to_owned(),
                })
            }
            _ => None,
        })
}

fn ensure_video_frame_size(length: usize) -> Result<(), RetinaRtspError> {
    if length > RETINA_MAX_VIDEO_FRAME_BYTES {
        return Err(RetinaRtspError::VideoFrameTooLarge {
            max: RETINA_MAX_VIDEO_FRAME_BYTES,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test(flavor = "current_thread")]
    async fn rtsp_session_rejects_non_rtsp_or_authorityless_uris_before_network_io() {
        assert!(matches!(
            RetinaRtspPreviewSession::connect("http://192.168.1.254/xxx.mov").await,
            Err(RetinaRtspError::InvalidUri)
        ));
        assert!(matches!(
            RetinaRtspPreviewSession::connect("rtsp:///xxx.mov").await,
            Err(RetinaRtspError::InvalidUri)
        ));
        assert!(matches!(
            RetinaRtspPreviewSession::connect("rtsp://example.com/xxx.mov").await,
            Err(RetinaRtspError::NonLocalUri)
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rtsp_session_rejects_a_uri_from_a_different_local_origin_before_network_io() {
        assert!(matches!(
            RetinaRtspPreviewSession::connect_for_origin(
                "rtsp://192.168.1.253/xxx.mov",
                Ipv4Addr::new(192, 168, 1, 254),
            )
            .await,
            Err(RetinaRtspError::OriginMismatch)
        ));
    }

    #[test]
    fn h264_file_sink_writes_annex_b_access_units() {
        let path = std::env::temp_dir().join(format!("cutout-retina-{}.h264", std::process::id()));
        let frame = RetinaVideoFrame {
            data: [0u8, 0, 0, 2, 0x67, 0x01, 0, 0, 0, 1, 0x65].to_vec(),
            loss: 0,
            is_random_access_point: true,
            timestamp: 90_000,
            clock_rate_hz: 90_000,
        };
        assert_eq!(frame.timestamp, 90_000);
        assert_eq!(frame.clock_rate_hz, 90_000);

        let mut sink = RetinaH264FileSink::create(&path).unwrap();
        sink.write_frame(&frame).unwrap();
        sink.finish().unwrap();

        assert_eq!(
            fs::read(&path).unwrap(),
            [0, 0, 0, 1, 0x67, 0x01, 0, 0, 0, 1, 0x65]
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn h264_file_sink_rejects_truncated_length_prefix() {
        let path = std::env::temp_dir().join(format!(
            "cutout-retina-truncated-{}.h264",
            std::process::id()
        ));
        let frame = RetinaVideoFrame {
            data: vec![0, 0, 0],
            loss: 0,
            is_random_access_point: false,
            timestamp: 0,
            clock_rate_hz: 90_000,
        };

        let mut sink = RetinaH264FileSink::create(&path).unwrap();
        let error = sink.write_frame(&frame).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        drop(sink);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rtsp_frame_size_is_bounded_before_crossing_the_mobile_boundary() {
        assert!(ensure_video_frame_size(RETINA_MAX_VIDEO_FRAME_BYTES).is_ok());
        assert!(matches!(
            ensure_video_frame_size(RETINA_MAX_VIDEO_FRAME_BYTES + 1),
            Err(RetinaRtspError::VideoFrameTooLarge { .. })
        ));
    }
}
