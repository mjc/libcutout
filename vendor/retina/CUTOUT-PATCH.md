# Cutout patch to Retina 0.4.21

Source: crates.io Retina 0.4.21, upstream commit
`66180147ac138efcaf478bd8730f3d4a272f3c29`.
Crate archive SHA-256:
`031b39326ce8613be3273edaac2d132709a08790f73c4dee446a781f6c990e7c`.
The archive checksum matches the crates.io registry index entry for 0.4.21.
The upstream original manifest (`Cargo.toml.orig`), README and both upstream
licenses are retained. The active manifest and API ports are described below.
Registry metadata and its independent lockfile are omitted.

`Session::describe_with_tcp_stream` accepts a connected Tokio TCP stream and
delegates to the existing RTSP session implementation. Cutout validates the URI
and peer address before handing it over. Android binds the socket to the camera's
Wi-Fi network before connecting, without changing the process's default network.

`TeardownPolicy::ExistingConnection` sends one cleanup request over that same
socket and disables fresh-connection retries, including for old live555 servers.
Those retries would bypass the native network binding.

RTSP framing, authentication, RTP and codecs retain the upstream algorithms
with the dependency API ports below.
Remove this patch when an upstream release supports both connection injection
and cleanup that cannot reconnect through the default network.

The active manifest selects the latest stable direct dependencies, including
bitstream-io 4, h264-reader 0.9, rand 0.10, sdp-types 0.2, socket2 0.6 and
Criterion 0.8. `Cargo.toml.orig` remains the original 0.4.21 release manifest.
AAC fixed-width reads use bitstream-io's const-width API. H.26x bit-reader
adapters implement h264-reader's fixed/variable read methods, `read_bit` and
`byte_aligned`, retaining trailing-data tolerance. UDP port selection uses
rand's `rng` and `random_range` APIs. Existing cleanup and supplied-socket
patches remain unchanged.

The registry manifest omitted upstream's Git-only `mylog` dev dependency while
retaining tests that use it. The active manifest restores that dependency at
commit `78a46b9c98521e32b0db04bdb5e026b11609b925` so the standalone test suite
can compile reproducibly. The upstream original manifest remains unchanged.
