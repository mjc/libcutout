//! RTSP preview transport for camera sources.

use std::{
    fs::File,
    future::Future,
    io::{self, BufWriter, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    num::NonZeroU32,
    path::Path,
    time::Duration,
};

use crate::NovatekHttpOrigin;
use arrayvec::ArrayString;
use futures_util::StreamExt;
use retina::{
    client,
    codec::{CodecItem, ParametersRef},
};
use thiserror::Error;
use url::Url;

/// Maximum encoded H.264 access-unit size accepted from an RTSP camera.
const MAX_VIDEO_FRAME_BYTES: usize = 8 * 1024 * 1024;
const MAX_VIDEO_CONFIGURATION_BYTES: usize = 64 * 1024;
const MAX_VIDEO_CODEC_BYTES: usize = 64;
const RTSP_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const RTSP_IDLE_TIMEOUT: Duration = Duration::from_secs(15);

/// Failure while creating or consuming an RTSP preview session.
#[derive(Debug, Error)]
pub enum RtspError {
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
    /// RTSP could not establish or negotiate the session.
    #[error("RTSP session error: {0}")]
    Session(String),
    /// A decoded access unit exceeded the mobile preview memory budget.
    #[error("RTSP video frame exceeds {max} bytes")]
    VideoFrameTooLarge {
        /// Maximum accepted encoded frame size.
        max: usize,
    },
}

/// A nonzero RTP clock rate associated with an encoded video frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VideoClockRate(NonZeroU32);

impl VideoClockRate {
    /// Creates a clock rate, rejecting zero because it cannot be a time scale.
    #[must_use]
    pub const fn new(value: u32) -> Option<Self> {
        match NonZeroU32::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    const fn from_nonzero(value: NonZeroU32) -> Self {
        Self(value)
    }

    /// Returns the numeric clock rate in hertz for an FFI boundary.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

/// Failure while constructing a bounded encoded video frame.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum VideoFrameError {
    /// The encoded access unit was not a complete length-prefixed H.264 unit.
    #[error("RTSP video frame is not a valid H.264 access unit")]
    InvalidAccessUnit,
    /// The encoded access unit exceeded the fixed frame bound.
    #[error("RTSP video frame exceeds {max} bytes")]
    TooLarge {
        /// Maximum accepted encoded frame size.
        max: usize,
    },
}

/// One bounded encoded video access unit emitted by a RTSP preview session.
#[derive(Debug, Eq, PartialEq)]
pub struct VideoFrame {
    data: Vec<u8>,
    parameter_sets: Vec<Vec<u8>>,
    loss: u16,
    is_random_access_point: bool,
    timestamp: i64,
    clock_rate_hz: VideoClockRate,
}

/// Named components of a validated encoded video frame for a mobile boundary.
#[derive(Debug, Eq, PartialEq)]
pub struct VideoFrameParts {
    /// Encoded length-prefixed H.264 access-unit bytes.
    pub data: Vec<u8>,
    /// SPS/PPS NAL units discovered in the access unit.
    pub parameter_sets: Vec<Vec<u8>>,
    /// RTP packets lost before this frame.
    pub loss: u16,
    /// Whether this frame is a random-access point.
    pub is_random_access_point: bool,
    /// Presentation timestamp in the stream clock.
    pub timestamp: i64,
    /// Nonzero RTP clock rate associated with the timestamp.
    pub clock_rate_hz: VideoClockRate,
}

/// A bounded RFC 6381 codec identifier advertised by an RTSP stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoCodec(ArrayString<MAX_VIDEO_CODEC_BYTES>);

impl VideoCodec {
    /// Retains a codec identifier only when it fits the descriptor bound.
    #[must_use]
    pub fn new(value: &str) -> Option<Self> {
        ArrayString::try_from(value).ok().map(Self)
    }

    /// Returns the bounded codec identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Nonzero coded dimensions advertised by an RTSP video stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VideoDimensions {
    width: NonZeroU32,
    height: NonZeroU32,
}

impl VideoDimensions {
    /// Creates dimensions, rejecting a missing coded width or height.
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Option<Self> {
        let (Some(width), Some(height)) = (NonZeroU32::new(width), NonZeroU32::new(height)) else {
            return None;
        };
        Some(Self { width, height })
    }

    /// Returns the coded width in pixels.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width.get()
    }

    /// Returns the coded height in pixels.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height.get()
    }
}

/// Failure while constructing a bounded RTSP video configuration.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum VideoConfigurationError {
    /// The stream advertised a zero coded dimension.
    #[error("RTSP video dimensions must be nonzero")]
    InvalidDimensions,
    /// The codec-specific configuration exceeded the fixed bound.
    #[error("RTSP codec configuration exceeds {max} bytes")]
    ExtraDataTooLarge {
        /// Maximum accepted codec-specific configuration size.
        max: usize,
    },
}

/// Codec configuration advertised by the RTSP video stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoConfiguration {
    codec: VideoCodec,
    dimensions: VideoDimensions,
    extra_data: Vec<u8>,
}

impl VideoConfiguration {
    /// Creates a bounded codec configuration from stream metadata.
    ///
    /// # Errors
    ///
    /// Returns [`VideoConfigurationError::InvalidDimensions`] for a zero
    /// coded width or height, or [`VideoConfigurationError::ExtraDataTooLarge`]
    /// when codec-specific data exceeds the fixed boundary.
    pub fn new(
        codec: VideoCodec,
        width: u32,
        height: u32,
        extra_data: Vec<u8>,
    ) -> Result<Self, VideoConfigurationError> {
        let dimensions = VideoDimensions::new(width, height)
            .ok_or(VideoConfigurationError::InvalidDimensions)?;
        if extra_data.len() > MAX_VIDEO_CONFIGURATION_BYTES {
            return Err(VideoConfigurationError::ExtraDataTooLarge {
                max: MAX_VIDEO_CONFIGURATION_BYTES,
            });
        }
        Ok(Self {
            codec,
            dimensions,
            extra_data,
        })
    }

    /// Returns the bounded RFC 6381 codec identifier.
    #[must_use]
    pub fn codec(&self) -> &VideoCodec {
        &self.codec
    }

    /// Returns the nonzero coded dimensions.
    #[must_use]
    pub const fn dimensions(&self) -> VideoDimensions {
        self.dimensions
    }

    /// Returns the codec-specific decoder configuration without copying.
    #[must_use]
    pub fn extra_data(&self) -> &[u8] {
        &self.extra_data
    }
}

impl VideoFrame {
    /// Creates one encoded video access unit with its stream timing.
    ///
    /// # Errors
    ///
    /// Returns [`VideoFrameError::TooLarge`] when the encoded access
    /// unit exceeds the fixed transport boundary.
    pub fn new(
        data: Vec<u8>,
        loss: u16,
        is_random_access_point: bool,
        timestamp: i64,
        clock_rate_hz: VideoClockRate,
    ) -> Result<Self, VideoFrameError> {
        if data.len() > MAX_VIDEO_FRAME_BYTES {
            return Err(VideoFrameError::TooLarge {
                max: MAX_VIDEO_FRAME_BYTES,
            });
        }
        let nals = parse_length_prefixed_access_unit(&data)
            .map_err(|_| VideoFrameError::InvalidAccessUnit)?;
        let sps = nals
            .iter()
            .find(|nal| nal.first().is_some_and(|header| header & 0x1f == 7))
            .map(|nal| nal.to_vec());
        let pps = nals
            .iter()
            .find(|nal| nal.first().is_some_and(|header| header & 0x1f == 8))
            .map(|nal| nal.to_vec());
        let parameter_sets = [sps, pps].into_iter().flatten().collect();
        Ok(Self {
            data,
            parameter_sets,
            loss,
            is_random_access_point,
            timestamp,
            clock_rate_hz,
        })
    }

    /// Returns the encoded access-unit bytes without copying.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Returns SPS/PPS NAL units found in this validated access unit.
    #[must_use]
    pub fn parameter_sets(&self) -> &[Vec<u8>] {
        &self.parameter_sets
    }

    /// Returns the number of lost RTP packets before this frame.
    #[must_use]
    pub const fn loss(&self) -> u16 {
        self.loss
    }

    /// Returns whether this frame is a random-access point.
    #[must_use]
    pub const fn is_random_access_point(&self) -> bool {
        self.is_random_access_point
    }

    /// Returns the presentation timestamp in the stream's clock units.
    #[must_use]
    pub const fn timestamp(&self) -> i64 {
        self.timestamp
    }

    /// Returns the nonzero stream clock rate.
    #[must_use]
    pub const fn clock_rate_hz(&self) -> VideoClockRate {
        self.clock_rate_hz
    }

    /// Splits the validated frame for a checked FFI DTO conversion.
    #[must_use]
    pub fn into_parts(self) -> VideoFrameParts {
        VideoFrameParts {
            data: self.data,
            parameter_sets: self.parameter_sets,
            loss: self.loss,
            is_random_access_point: self.is_random_access_point,
            timestamp: self.timestamp,
            clock_rate_hz: self.clock_rate_hz,
        }
    }
}

/// A file sink for RTSP's H.264 access units.
///
/// The output is an Annex-B H.264 elementary stream. It is intentionally not
/// an MP4/MOV container: callers can hand the resulting file to a decoder or
/// muxer that understands H.264 elementary streams, while the transport layer
/// remains independent of platform media frameworks.
pub struct H264FileSink {
    writer: BufWriter<File>,
    frame_count: u64,
}

impl std::fmt::Debug for H264FileSink {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("H264FileSink")
            .field("frame_count", &self.frame_count)
            .finish_non_exhaustive()
    }
}

impl H264FileSink {
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

    /// Writes one RTSP H.264 access unit to the output file.
    ///
    /// RTSP emits each NAL unit with a four-byte big-endian length prefix;
    /// this sink replaces those prefixes with Annex-B start codes.
    ///
    /// # Errors
    ///
    /// Returns [`io::ErrorKind::InvalidData`] for malformed length-prefixed
    /// data, or the underlying filesystem error when writing.
    pub fn write_frame(&mut self, frame: &VideoFrame) -> io::Result<()> {
        let data = frame.data();
        let nals = parse_length_prefixed_access_unit(data).map_err(invalid_data)?;

        for nal in nals {
            self.writer.write_all(&[0, 0, 0, 1])?;
            self.writer.write_all(nal)?;
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

fn parse_length_prefixed_access_unit(data: &[u8]) -> Result<Vec<&[u8]>, &'static str> {
    if data.is_empty() {
        return Err("H.264 access unit is empty");
    }

    let mut cursor = 0;
    let mut nals = Vec::new();
    while cursor < data.len() {
        let nal_length =
            read_nal_length(data, cursor).map_err(|_| "H.264 NAL length is invalid")?;
        cursor += 4;
        if nal_length == 0 || nal_length > data.len() - cursor {
            return Err("H.264 NAL length exceeds access unit");
        }
        let nal = data
            .get(cursor..cursor + nal_length)
            .ok_or("H.264 NAL range is invalid")?;
        nals.push(nal);
        cursor += nal_length;
    }
    Ok(nals)
}

/// Extracts the first SPS and PPS NAL units from an H.264 `avcC` record.
#[must_use]
pub fn parse_avcc_parameter_sets(data: &[u8]) -> Option<Vec<Vec<u8>>> {
    if data.first().copied()? != 1 {
        return None;
    }

    let mut cursor = 6;
    let sps_count = usize::from(*data.get(5)? & 0x1f);
    let sps = read_avcc_parameter_set(data, &mut cursor, sps_count)?;
    let pps_count = usize::from(*data.get(cursor)?);
    cursor += 1;
    let pps = read_avcc_parameter_set(data, &mut cursor, pps_count)?;
    Some(vec![sps, pps])
}

fn read_avcc_parameter_set(data: &[u8], cursor: &mut usize, count: usize) -> Option<Vec<u8>> {
    let mut first = None;
    for _ in 0..count {
        let length = usize::from(u16::from_be_bytes([
            *data.get(*cursor)?,
            *data.get(*cursor + 1)?,
        ]));
        *cursor += 2;
        let end = (*cursor).checked_add(length)?;
        let nal = data.get(*cursor..end)?;
        if nal.is_empty() {
            return None;
        }
        first.get_or_insert_with(|| nal.to_vec());
        *cursor = end;
    }
    first
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

/// A running RTSP RTSP session restricted to the first advertised video stream.
///
/// The session owns the transport and ends when dropped. It deliberately does
/// not impose an application timeout: callers stop it by dropping the session
/// or cancelling the task that is awaiting [`Self::next_video_frame`].
pub struct RtspPreviewSession {
    demuxed: client::Demuxed,
    video_stream_id: usize,
    video_configuration: Option<VideoConfiguration>,
}

impl std::fmt::Debug for RtspPreviewSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RtspPreviewSession")
            .field("video_stream_id", &self.video_stream_id)
            .finish_non_exhaustive()
    }
}

impl RtspPreviewSession {
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
    ) -> Result<Self, RtspError> {
        Self::connect_for_origin_with_connector(uri, expected_address, |address| async move {
            tokio::net::TcpStream::connect(address)
                .await
                .map_err(|error| RtspError::Session(error.to_string()))
        })
        .await
    }

    /// Connects through a native-selected network while retaining origin validation.
    ///
    /// The connector runs only after URI validation and must return a stream to
    /// that exact address and port. The entire connection and handshake share the
    /// existing timeout. Native callers can bind an unconnected socket without
    /// changing process-wide networking.
    ///
    /// # Errors
    ///
    /// Returns the same validation and negotiation errors as [`Self::connect_for_origin`],
    /// including a connector that returns a stream to a different peer.
    pub async fn connect_for_origin_with_connector<F, C>(
        uri: &str,
        expected_address: Ipv4Addr,
        connect: F,
    ) -> Result<Self, RtspError>
    where
        F: FnOnce(SocketAddrV4) -> C,
        C: Future<Output = Result<tokio::net::TcpStream, RtspError>>,
    {
        tokio::time::timeout(
            RTSP_HANDSHAKE_TIMEOUT,
            Self::connect_inner_unbounded(uri, expected_address, connect),
        )
        .await
        .map_err(|_| RtspError::Session("RTSP handshake timed out".to_owned()))?
    }

    async fn connect_inner_unbounded<F, C>(
        uri: &str,
        expected_address: Ipv4Addr,
        connect: F,
    ) -> Result<Self, RtspError>
    where
        F: FnOnce(SocketAddrV4) -> C,
        C: Future<Output = Result<tokio::net::TcpStream, RtspError>>,
    {
        let url = Url::parse(uri).map_err(|_| RtspError::InvalidUri)?;
        if url.scheme() != "rtsp"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(RtspError::InvalidUri);
        }
        let host = url
            .host_str()
            .and_then(|host| host.parse::<Ipv4Addr>().ok())
            .ok_or(RtspError::NonLocalUri)?;
        if expected_address != host {
            return Err(RtspError::OriginMismatch);
        }
        let address = SocketAddrV4::new(host, url.port().unwrap_or(554));
        NovatekHttpOrigin::new(host, address.port()).map_err(|_| RtspError::NonLocalUri)?;

        let stream = connect(address).await?;
        if stream
            .peer_addr()
            .map_err(|error| RtspError::Session(error.to_string()))?
            != SocketAddr::V4(address)
        {
            return Err(RtspError::OriginMismatch);
        }
        let mut described = client::Session::describe_with_tcp_stream(
            url,
            client::SessionOptions::default().teardown(client::TeardownPolicy::ExistingConnection),
            stream,
        )
        .await
        .map_err(|error| RtspError::Session(error.to_string()))?;
        let video_stream_id = described
            .streams()
            .iter()
            .position(|stream| stream.media() == "video" && stream.encoding_name() == "h264")
            .ok_or(RtspError::UnsupportedVideoCodec)?;
        described
            .setup(
                video_stream_id,
                client::SetupOptions::default().transport(client::Transport::default()),
            )
            .await
            .map_err(|error| RtspError::Session(error.to_string()))?;
        let playing = described
            .play(client::PlayOptions::default())
            .await
            .map_err(|error| RtspError::Session(error.to_string()))?;
        let demuxed = playing
            .demuxed()
            .map_err(|error| RtspError::Session(error.to_string()))?;

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
    pub fn video_configuration(&self) -> Option<&VideoConfiguration> {
        self.video_configuration.as_ref()
    }

    /// Waits for the next encoded video access unit.
    ///
    /// # Errors
    ///
    /// Returns a RTSP error when the transport or depacketizer fails.
    pub async fn next_video_frame(&mut self) -> Result<Option<VideoFrame>, RtspError> {
        while let Some(item) = tokio::time::timeout(RTSP_IDLE_TIMEOUT, self.demuxed.next())
            .await
            .map_err(|_| RtspError::Session("RTSP stream idle timeout".to_owned()))?
        {
            match item.map_err(|error| RtspError::Session(error.to_string()))? {
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
                    let clock_rate_hz = VideoClockRate::from_nonzero(timestamp.clock_rate());
                    return Ok(Some(
                        VideoFrame::new(
                            data,
                            loss,
                            is_random_access_point,
                            timestamp.timestamp(),
                            clock_rate_hz,
                        )
                        .map_err(|error| match error {
                            VideoFrameError::TooLarge { max } => {
                                RtspError::VideoFrameTooLarge { max }
                            }
                            VideoFrameError::InvalidAccessUnit => {
                                RtspError::Session("invalid H.264 access unit".to_owned())
                            }
                        })?,
                    ));
                }
                _ => {}
            }
        }
        Ok(None)
    }
}

fn video_configuration_for_stream(stream: Option<&client::Stream>) -> Option<VideoConfiguration> {
    stream
        .and_then(client::Stream::parameters)
        .and_then(|parameters| match parameters {
            ParametersRef::Video(parameters) => VideoCodec::new(parameters.rfc6381_codec())
                .and_then(|codec| {
                    let (width, height) = parameters.pixel_dimensions();
                    VideoConfiguration::new(
                        codec,
                        width,
                        height,
                        parameters.extra_data().to_owned(),
                    )
                    .ok()
                }),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::{Read, Write},
        net::{Shutdown, TcpListener},
        thread,
    };

    async fn read_rtsp_request_cseq(
        connection: &mut tokio::io::BufReader<tokio::net::TcpStream>,
        method: &str,
    ) -> String {
        use tokio::io::AsyncBufReadExt;

        let mut request = String::new();
        loop {
            let mut line = String::new();
            assert_ne!(connection.read_line(&mut line).await.unwrap(), 0);
            request.push_str(&line);
            if line == "\r\n" {
                break;
            }
        }
        assert!(request.starts_with(method), "{request}");
        request
            .lines()
            .find_map(|line| line.strip_prefix("CSeq: "))
            .unwrap()
            .to_owned()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rtsp_session_rejects_a_truncated_describe_body() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("local listener binds");
        let address = listener.local_addr().expect("listener has a local address");
        let server = thread::spawn(move || {
            let (mut connection, _) = listener.accept().expect("client connects");
            let mut request = [0; 1024];
            let request_len = connection.read(&mut request).expect("client sends request");
            let request = std::str::from_utf8(&request[..request_len]).expect("request is UTF-8");
            let cseq = request
                .lines()
                .find_map(|line| line.strip_prefix("CSeq: "))
                .expect("request has a CSeq");
            assert!(request.starts_with("DESCRIBE "));
            connection
                .write_all(
                    format!("RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\nContent-Type: application/sdp\r\nContent-Length: 64\r\n\r\nv=0\r\n").as_bytes(),
                )
                .expect("server sends partial response");
            connection
                .shutdown(Shutdown::Both)
                .expect("server closes response");
        });

        let result = RtspPreviewSession::connect_for_origin(
            &format!("rtsp://{address}/stream"),
            Ipv4Addr::LOCALHOST,
        )
        .await;

        assert!(matches!(result, Err(RtspError::Session(_))));
        server.join().expect("fake RTSP server completes");
    }

    #[tokio::test]
    async fn rtsp_cleanup_never_reconnects_after_a_failed_teardown() {
        use tokio::io::{AsyncWriteExt, BufReader};

        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (connection, _) = listener.accept().await.unwrap();
            let mut connection = BufReader::new(connection);
            for method in ["DESCRIBE", "SETUP", "PLAY", "TEARDOWN"] {
                let cseq = read_rtsp_request_cseq(&mut connection, method).await;
                let response = match method {
                    "DESCRIBE" => {
                        // This old live555 version triggers Retina Auto's fresh-connection
                        // cleanup retries; the bound-socket policy must suppress them.
                        let sdp = "v=0\r\no=- 1 1 IN IP4 127.0.0.1\r\ns=Camera\r\nt=0 0\r\na=tool:LIVE555 Streaming Media v2013.04.08\r\na=control:*\r\nm=video 0 RTP/AVP 96\r\nc=IN IP4 0.0.0.0\r\na=rtpmap:96 H264/90000\r\na=fmtp:96 packetization-mode=1;profile-level-id=640033;sprop-parameter-sets=Z2QAM6zoAoALWQ==,aO48MA==\r\na=control:trackID=1\r\n";
                        format!(
                            "RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\nContent-Type: application/sdp\r\nContent-Length: {}\r\n\r\n{sdp}",
                            sdp.len()
                        )
                    }
                    "SETUP" => format!(
                        "RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\nSession: camera\r\nTransport: RTP/AVP/TCP;unicast;interleaved=0-1\r\n\r\n"
                    ),
                    "PLAY" => format!(
                        "RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\nSession: camera\r\nRange: npt=0.000-\r\nRTP-Info: url=trackID=1;seq=1;rtptime=1\r\n\r\n"
                    ),
                    _ => format!("RTSP/1.0 503 Unavailable\r\nCSeq: {cseq}\r\n\r\n"),
                };
                connection
                    .get_mut()
                    .write_all(response.as_bytes())
                    .await
                    .unwrap();
            }
            drop(connection);
            assert!(
                tokio::time::timeout(Duration::from_millis(500), listener.accept())
                    .await
                    .is_err(),
                "cleanup opened an unbound connection"
            );
        });
        let session = RtspPreviewSession::connect_for_origin(
            &format!("rtsp://{address}/live"),
            Ipv4Addr::LOCALHOST,
        )
        .await
        .unwrap();
        drop(session);
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn rtsp_keyframe_keeps_sdp_parameter_sets_for_native_decoders_and_file_sink() {
        use tokio::io::{AsyncWriteExt, BufReader};

        // The same R3 SPS/PPS and IDR NAL used by the native renderer fixture.
        // Parameter sets travel only in SDP, never in the RTP payload.
        const SPS: &[u8] = &[
            0x67, 0x64, 0x00, 0x33, 0xac, 0x15, 0x4a, 0x0d, 0x43, 0xda, 0x6e, 0x02, 0x02, 0x02,
            0x80, 0x00, 0x01, 0xf4, 0x00, 0x00, 0x75, 0x30, 0x02,
        ];
        const PPS: &[u8] = &[0x68, 0xee, 0x3c, 0xb0];
        const IDR: &[u8] = &[0x65, 0x88, 0x80];
        const START_CODE: &[u8] = &[0, 0, 0, 1];

        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (connection, _) = listener.accept().await.unwrap();
            let mut connection = BufReader::new(connection);
            for method in ["DESCRIBE", "SETUP", "PLAY", "TEARDOWN"] {
                let cseq = read_rtsp_request_cseq(&mut connection, method).await;
                let response = match method {
                    "DESCRIBE" => {
                        let sdp = "v=0\r\no=- 1 1 IN IP4 127.0.0.1\r\ns=Camera\r\nt=0 0\r\na=control:*\r\nm=video 0 RTP/AVP 96\r\nc=IN IP4 0.0.0.0\r\na=rtpmap:96 H264/90000\r\na=fmtp:96 packetization-mode=1;profile-level-id=640033;sprop-parameter-sets=Z2QAM6wVSg1D2m4CAgKAAAH0AAB1MAI=,aO48sA==\r\na=control:trackID=1\r\n";
                        format!(
                            "RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\nContent-Type: application/sdp\r\nContent-Length: {}\r\n\r\n{sdp}",
                            sdp.len()
                        )
                    }
                    "SETUP" => format!(
                        "RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\nSession: camera\r\nTransport: RTP/AVP/TCP;unicast;interleaved=0-1\r\n\r\n"
                    ),
                    "PLAY" => format!(
                        "RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\nSession: camera\r\nRange: npt=0.000-\r\nRTP-Info: url=trackID=1;seq=1;rtptime=90000\r\n\r\n"
                    ),
                    _ => format!("RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\n\r\n"),
                };
                connection
                    .get_mut()
                    .write_all(response.as_bytes())
                    .await
                    .unwrap();
                if method == "PLAY" {
                    // Interleaved channel 0, RTP version 2, marker set, PT 96,
                    // sequence 1, timestamp 90000, SSRC 0x12345678.
                    let mut packet = vec![b'$', 0];
                    let rtp_len = u16::try_from(12 + IDR.len()).unwrap();
                    packet.extend_from_slice(&rtp_len.to_be_bytes());
                    packet.extend_from_slice(&[
                        0x80, 0xe0, 0, 1, 0, 1, 0x5f, 0x90, 0x12, 0x34, 0x56, 0x78,
                    ]);
                    packet.extend_from_slice(IDR);
                    connection.get_mut().write_all(&packet).await.unwrap();
                }
            }
        });

        let mut session = RtspPreviewSession::connect_for_origin(
            &format!("rtsp://{address}/live"),
            Ipv4Addr::LOCALHOST,
        )
        .await
        .unwrap();
        let configuration = session.video_configuration().unwrap();
        assert_eq!(
            parse_avcc_parameter_sets(configuration.extra_data()),
            Some(vec![SPS.to_vec(), PPS.to_vec()])
        );
        let frame = session.next_video_frame().await.unwrap().unwrap();
        assert!(frame.is_random_access_point());
        assert_eq!(frame.loss(), 0);
        assert_eq!(frame.timestamp(), 90_000);
        assert_eq!(frame.clock_rate_hz().get(), 90_000);
        assert_eq!(frame.parameter_sets(), &[SPS.to_vec(), PPS.to_vec()]);
        assert_eq!(
            parse_length_prefixed_access_unit(frame.data()).unwrap(),
            vec![SPS, PPS, IDR]
        );

        let path = std::env::temp_dir().join(format!(
            "cutout-camera-sdp-keyframe-{}.h264",
            std::process::id()
        ));
        let mut sink = H264FileSink::create(&path).unwrap();
        sink.write_frame(&frame).unwrap();
        sink.finish().unwrap();
        let saved = fs::read(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(
            saved,
            [START_CODE, SPS, START_CODE, PPS, START_CODE, IDR].concat()
        );

        drop(session);
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn rtsp_connector_runs_only_after_origin_validation() {
        let calls = std::cell::Cell::new(0);
        for uri in [
            "http://192.168.1.254/live",
            "rtsp://example.com/live",
            "rtsp://192.168.1.253/live",
            "rtsp://name:password@192.168.1.254/live",
        ] {
            let result = RtspPreviewSession::connect_for_origin_with_connector(
                uri,
                Ipv4Addr::new(192, 168, 1, 254),
                |_| {
                    calls.set(calls.get() + 1);
                    std::future::ready(Err(RtspError::Session("unexpected connection".to_owned())))
                },
            )
            .await;
            assert!(result.is_err());
        }
        assert_eq!(calls.get(), 0);
    }

    #[tokio::test]
    async fn rtsp_connector_cannot_redirect_to_a_different_peer() {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();
        let (mut peer, _) = listener.accept().await.unwrap();
        let expected_port = if address.port() == 554 { 555 } else { 554 };
        let result = RtspPreviewSession::connect_for_origin_with_connector(
            &format!("rtsp://127.0.0.1:{expected_port}/live"),
            Ipv4Addr::LOCALHOST,
            |_| std::future::ready(Ok(stream)),
        )
        .await;
        assert!(matches!(result, Err(RtspError::OriginMismatch)));
        let mut bytes = [0u8; 1];
        assert_eq!(
            tokio::io::AsyncReadExt::read(&mut peer, &mut bytes)
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test(start_paused = true)]
    async fn rtsp_connector_uses_the_same_handshake_deadline() {
        let result = RtspPreviewSession::connect_for_origin_with_connector(
            "rtsp://127.0.0.1/live",
            Ipv4Addr::LOCALHOST,
            |_| std::future::pending::<Result<tokio::net::TcpStream, RtspError>>(),
        )
        .await;
        assert!(
            matches!(result, Err(RtspError::Session(message)) if message == "RTSP handshake timed out")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rtsp_session_rejects_non_rtsp_or_authorityless_uris_before_network_io() {
        assert!(matches!(
            RtspPreviewSession::connect_for_origin(
                "http://192.168.1.254/xxx.mov",
                Ipv4Addr::new(192, 168, 1, 254),
            )
            .await,
            Err(RtspError::InvalidUri)
        ));
        assert!(matches!(
            RtspPreviewSession::connect_for_origin(
                "rtsp:///xxx.mov",
                Ipv4Addr::new(192, 168, 1, 254),
            )
            .await,
            Err(RtspError::InvalidUri)
        ));
        assert!(matches!(
            RtspPreviewSession::connect_for_origin(
                "rtsp://example.com/xxx.mov",
                Ipv4Addr::new(192, 168, 1, 254),
            )
            .await,
            Err(RtspError::NonLocalUri)
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rtsp_session_rejects_a_uri_from_a_different_local_origin_before_network_io() {
        assert!(matches!(
            RtspPreviewSession::connect_for_origin(
                "rtsp://192.168.1.253/xxx.mov",
                Ipv4Addr::new(192, 168, 1, 254),
            )
            .await,
            Err(RtspError::OriginMismatch)
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rtsp_session_rejects_uri_userinfo_before_network_io() {
        assert!(matches!(
            RtspPreviewSession::connect_for_origin(
                "rtsp://camera-user:camera-password@192.168.1.254/xxx.mov",
                Ipv4Addr::new(192, 168, 1, 254),
            )
            .await,
            Err(RtspError::InvalidUri)
        ));
    }

    #[test]
    fn h264_file_sink_writes_annex_b_access_units() {
        let path = std::env::temp_dir().join(format!("cutout-camera-{}.h264", std::process::id()));
        let frame = VideoFrame::new(
            [0u8, 0, 0, 2, 0x67, 0x01, 0, 0, 0, 1, 0x65].to_vec(),
            0,
            true,
            90_000,
            VideoClockRate::new(90_000).unwrap(),
        )
        .unwrap();
        assert_eq!(frame.timestamp(), 90_000);
        assert_eq!(frame.clock_rate_hz().get(), 90_000);
        assert_eq!(frame.parameter_sets(), &[vec![0x67, 0x01]]);

        let mut sink = H264FileSink::create(&path).unwrap();
        sink.write_frame(&frame).unwrap();
        sink.finish().unwrap();

        assert_eq!(
            fs::read(&path).unwrap(),
            [0, 0, 0, 1, 0x67, 0x01, 0, 0, 0, 1, 0x65]
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn video_frame_parts_keep_named_timing_types_at_the_ffi_boundary() {
        let frame = VideoFrame::new(
            [0u8, 0, 0, 1, 0x65].to_vec(),
            2,
            true,
            90_000,
            VideoClockRate::new(90_000).unwrap(),
        )
        .unwrap();

        let parts = frame.into_parts();

        assert_eq!(parts.loss, 2);
        assert!(parts.is_random_access_point);
        assert_eq!(parts.timestamp, 90_000);
        assert_eq!(parts.clock_rate_hz.get(), 90_000);
    }

    #[test]
    fn h264_frame_rejects_truncated_length_prefix() {
        let frame = VideoFrame::new(
            vec![0, 0, 0],
            0,
            false,
            0,
            VideoClockRate::new(90_000).unwrap(),
        );
        assert_eq!(frame, Err(VideoFrameError::InvalidAccessUnit));
    }

    #[test]
    fn avcc_configuration_extracts_first_sps_and_pps() {
        let configuration = [
            1, 0x64, 0, 0x1f, 0xff, 0xe2, 0, 2, 0x67, 0x64, 0, 2, 0x67, 0x65, 2, 0, 2, 0x68, 0xee,
            0, 2, 0x68, 0xef,
        ];
        assert_eq!(
            parse_avcc_parameter_sets(&configuration),
            Some(vec![vec![0x67, 0x64], vec![0x68, 0xee]])
        );
    }

    #[test]
    fn avcc_configuration_rejects_truncated_parameter_sets() {
        assert_eq!(
            parse_avcc_parameter_sets(&[1, 0, 0, 0, 0, 1, 0, 2, 0x67]),
            None
        );
    }

    #[test]
    fn avcc_configuration_rejects_empty_sps_or_pps() {
        let empty_sequence_parameter_set = [1, 100, 0, 31, 0xff, 0xe1, 0, 0, 1, 0, 2, 0x68, 0];
        let empty_picture_parameter_set = [1, 100, 0, 31, 0xff, 0xe1, 0, 2, 0x67, 1, 0, 0];

        assert_eq!(
            parse_avcc_parameter_sets(&empty_sequence_parameter_set),
            None
        );
        assert_eq!(
            parse_avcc_parameter_sets(&empty_picture_parameter_set),
            None
        );
    }

    #[test]
    fn rtsp_codec_identifier_is_bounded() {
        let codec = "c".repeat(MAX_VIDEO_CODEC_BYTES);
        assert_eq!(VideoCodec::new(&codec).unwrap().as_str(), codec);
        assert!(VideoCodec::new(&format!("{codec}c")).is_none());
    }

    #[test]
    fn rtsp_video_clock_rate_rejects_zero() {
        assert!(VideoClockRate::new(0).is_none());
    }

    #[test]
    fn rtsp_video_frame_constructor_rejects_oversized_data() {
        let data = vec![0; MAX_VIDEO_FRAME_BYTES + 1];
        assert_eq!(
            VideoFrame::new(data, 0, false, 0, VideoClockRate::new(90_000).unwrap(),),
            Err(VideoFrameError::TooLarge {
                max: MAX_VIDEO_FRAME_BYTES,
            })
        );
    }

    #[test]
    fn rtsp_video_configuration_rejects_invalid_metadata() {
        let codec = VideoCodec::new("avc1.4D401E").unwrap();
        assert_eq!(
            VideoConfiguration::new(codec, 0, 480, vec![1, 2, 3]),
            Err(VideoConfigurationError::InvalidDimensions)
        );

        let codec = VideoCodec::new("avc1.4D401E").unwrap();
        assert_eq!(
            VideoConfiguration::new(codec, 848, 480, vec![0; MAX_VIDEO_CONFIGURATION_BYTES + 1],),
            Err(VideoConfigurationError::ExtraDataTooLarge {
                max: MAX_VIDEO_CONFIGURATION_BYTES,
            })
        );
    }
}
