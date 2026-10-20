//! Native per-socket network selection for a Rust-owned RTSP preview.

use std::{net::SocketAddrV4, os::fd::AsRawFd, sync::Arc};

use cutout_protocols::{RtspError, RtspPreviewSession};
use tokio::net::{TcpSocket, TcpStream};

use crate::{
    MobileCameraPreviewError, MobileCameraPreviewSession, mobile_camera_preview_from_session,
};

/// Binds one unconnected TCP socket to the native camera network.
///
/// Rust retains ownership of the descriptor throughout this synchronous call.
/// Android must duplicate it with `ParcelFileDescriptor.fromFd`, bind that
/// duplicate with `Network.bindSocket`, and close only the duplicate. A native
/// implementation must not retain or close the supplied descriptor. Returning
/// false prevents connection; it must never fall back to the default network.
#[uniffi::export(callback_interface)]
pub trait MobileCameraSocketBinding: Send + Sync {
    /// Returns true only after binding to the same still-current camera network.
    fn bind_socket(&self, socket_fd: i32) -> bool;
}

/// Connects an RTSP preview through the selected native camera network.
///
/// URI/origin validation occurs before the binding callback. Rust owns the
/// socket, peer validation, negotiation, cancellation and handshake deadline.
/// The callback is a native network effect and never changes process routing.
///
/// # Errors
///
/// Returns an error for invalid origins, refused binding, peer mismatch or
/// failed/timed-out connection and RTSP negotiation.
#[uniffi::export(async_runtime = "tokio")]
pub async fn mobile_camera_preview_connect_with_socket_binding(
    uri: String,
    expected_address: String,
    binding: Box<dyn MobileCameraSocketBinding>,
) -> Result<Arc<MobileCameraPreviewSession>, MobileCameraPreviewError> {
    let expected_address = expected_address
        .parse()
        .map_err(|_| MobileCameraPreviewError::NonLocalUri)?;
    let session =
        RtspPreviewSession::connect_for_origin_with_connector(&uri, expected_address, |address| {
            connect_bound_socket(address, binding)
        })
        .await?;
    Ok(mobile_camera_preview_from_session(session))
}

async fn connect_bound_socket(
    address: SocketAddrV4,
    binding: Box<dyn MobileCameraSocketBinding>,
) -> Result<TcpStream, RtspError> {
    // Move the owner into the blocking effect. If the handshake is cancelled or
    // times out, its descriptor remains live until that callback returns; a
    // delayed callback can never observe a reused descriptor after early drop.
    let socket = tokio::task::spawn_blocking(move || {
        let socket = TcpSocket::new_v4().map_err(session_error)?;
        if !binding.bind_socket(socket.as_raw_fd()) {
            return Err(RtspError::Session(
                "Camera network binding was refused".to_owned(),
            ));
        }
        Ok(socket)
    })
    .await
    .map_err(session_error)??;
    socket.connect(address.into()).await.map_err(session_error)
}

fn session_error(error: impl std::fmt::Display) -> RtspError {
    RtspError::Session(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io,
        net::{Ipv4Addr, TcpListener},
        sync::atomic::{AtomicUsize, Ordering},
    };

    struct Binding {
        listener: TcpListener,
        calls: Arc<AtomicUsize>,
        accepts: bool,
    }

    impl MobileCameraSocketBinding for Binding {
        fn bind_socket(&self, socket_fd: i32) -> bool {
            assert!(socket_fd >= 0);
            assert_eq!(
                self.listener.accept().unwrap_err().kind(),
                io::ErrorKind::WouldBlock
            );
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.accepts
        }
    }

    #[tokio::test]
    async fn invalid_origin_never_invokes_native_binding() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let result = mobile_camera_preview_connect_with_socket_binding(
            "rtsp://192.168.1.253/live".to_owned(),
            "192.168.1.254".to_owned(),
            Box::new(Binding {
                listener,
                calls: calls.clone(),
                accepts: true,
            }),
        )
        .await;
        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn native_binding_precedes_connect_and_refusal_never_falls_back() {
        for accepts in [false, true] {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            listener.set_nonblocking(true).unwrap();
            let address = match listener.local_addr().unwrap() {
                std::net::SocketAddr::V4(address) => address,
                std::net::SocketAddr::V6(_) => unreachable!(),
            };
            let calls = Arc::new(AtomicUsize::new(0));
            let result = connect_bound_socket(
                address,
                Box::new(Binding {
                    listener: listener.try_clone().unwrap(),
                    calls: calls.clone(),
                    accepts,
                }),
            )
            .await;
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            if accepts {
                assert_eq!(result.unwrap().peer_addr().unwrap(), address.into());
                assert!(listener.accept().is_ok());
            } else {
                assert!(result.is_err());
                assert_eq!(
                    listener.accept().unwrap_err().kind(),
                    io::ErrorKind::WouldBlock
                );
            }
        }
    }
}
