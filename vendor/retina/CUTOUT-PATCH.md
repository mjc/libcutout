# Cutout patch to Retina 0.4.13

Source: crates.io Retina 0.4.13, upstream commit
`fa697c273de0c2ae8300c79d62dfbc78e5490a89`.
The original manifests, source, README and both upstream licenses are retained.
Registry build outputs and its independent lockfile are omitted.

`Session::describe_with_tcp_stream` accepts a connected Tokio TCP stream and
delegates to the existing RTSP session implementation. Cutout validates the URI
and peer address before handing it over. Android binds the socket to the camera's
Wi-Fi network before connecting, without changing the process's default network.

`TeardownPolicy::ExistingConnection` sends one cleanup request over that same
socket and disables fresh-connection retries, including for old live555 servers.
Those retries would bypass the native network binding.

RTSP framing, authentication, RTP and codecs use the upstream implementation.
Remove this patch when an upstream release supports both connection injection
and cleanup that cannot reconnect through the default network.
