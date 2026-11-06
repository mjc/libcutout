//! Exercises charging synchronization through the public application runtime and transport.

use cutout_application::charging::{
    account::AccountDeletionError, reports::ChargingContentReport,
    sync_runtime::ChargingSyncService, transport::ChargingHttpClient,
};
use libcutout_persistence::{CommunityAccountState, RideDatabase};
use std::{
    error::Error,
    io::{self, BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use uuid::Uuid;

type RequestQueue = Arc<Mutex<mpsc::Receiver<String>>>;

struct HttpFixture {
    origin: String,
    requests: RequestQueue,
    release_first: mpsc::Sender<()>,
    thread: JoinHandle<()>,
}

impl HttpFixture {
    fn start(responses: Vec<&'static str>, block_first: bool) -> io::Result<Self> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        let origin = format!("http://{}/", listener.local_addr()?);
        let (request_sender, request_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let thread = thread::spawn(move || {
            for (index, body) in responses.into_iter().enumerate() {
                let (mut stream, _) = accept_with_timeout(&listener)
                    .expect("fixture accepts request before its deadline");
                stream
                    .set_nonblocking(false)
                    .expect("fixture reads a complete request with its bounded timeout");
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .expect("fixture sets a bounded read timeout");
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .expect("fixture sets a bounded write timeout");
                let request = read_request(&stream).expect("fixture reads complete request");
                request_sender
                    .send(request)
                    .expect("test still receives fixture requests");
                if index == 0 && block_first {
                    release_receiver
                        .recv_timeout(Duration::from_secs(5))
                        .expect("test releases first response");
                }
                write_json_response(&mut stream, body).expect("fixture writes response");
            }
        });
        Ok(Self {
            origin,
            requests: Arc::new(Mutex::new(request_receiver)),
            release_first: release_sender,
            thread,
        })
    }

    async fn next_request(&self) -> Result<String, Box<dyn Error>> {
        let requests = Arc::clone(&self.requests);
        let request = tokio::task::spawn_blocking(move || {
            requests
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .recv_timeout(Duration::from_secs(3))
        })
        .await??;
        Ok(request)
    }

    fn join(self) -> Result<(), Box<dyn Error>> {
        self.thread
            .join()
            .map_err(|_| io::Error::other("HTTP fixture thread panicked"))?;
        Ok(())
    }
}

fn accept_with_timeout(listener: &TcpListener) -> io::Result<(TcpStream, std::net::SocketAddr)> {
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok(connection) => return Ok(connection),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "HTTP fixture did not receive a request before its deadline",
                    ));
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }
}

fn read_request(stream: &TcpStream) -> io::Result<String> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut content_length = 0;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        if header == "\r\n" || header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;
    Ok(request_line)
}

fn write_json_response(stream: &mut TcpStream, body: &str) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

fn open_test_database() -> Result<(PathBuf, RideDatabase), Box<dyn Error>> {
    let directory =
        std::env::temp_dir().join(format!("cutout-charging-runtime-{}", Uuid::new_v4()));
    std::fs::create_dir(&directory)?;
    let database = RideDatabase::open(&directory.join("rides.sqlite"))?;
    Ok((directory, database))
}

#[tokio::test]
async fn retained_runtime_serializes_sync_before_account_deletion() -> Result<(), Box<dyn Error>> {
    let (directory, database) = open_test_database()?;
    let fixture = HttpFixture::start(
        vec![
            r#"{"items":[],"next_cursor":0,"has_more":false}"#,
            r#"{"status":"completed","completed_at_ms":1700000000000}"#,
        ],
        true,
    )?;
    let client = Arc::new(ChargingHttpClient::loopback(&fixture.origin).expect("loopback client"));
    let service = Arc::new(ChargingSyncService::new(database.clone()));
    let sync = tokio::spawn({
        let client = Arc::clone(&client);
        let service = Arc::clone(&service);
        async move { service.synchronize(&client, &"a".repeat(64)).await }
    });

    let sync_request = fixture.next_request().await?;
    assert!(sync_request.starts_with("GET /v1/charging-spots/changes?after=0&limit=10 "));

    let deletion = tokio::spawn({
        let client = Arc::clone(&client);
        let service = Arc::clone(&service);
        async move {
            service
                .delete_community_account(&client, &"a".repeat(64))
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if database.community_account_state()? == CommunityAccountState::Deleting {
                return Ok::<(), libcutout_persistence::StorageError>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    assert!(
        fixture
            .requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .try_recv()
            .is_err()
    );

    fixture.release_first.send(()).map_err(io::Error::other)?;
    let _summary = sync.await??;
    let deletion_request = fixture.next_request().await?;
    assert!(deletion_request.starts_with("DELETE /v1/identities/current "));
    assert_eq!(deletion.await??, 1_700_000_000_000);
    assert_eq!(
        database.community_account_state()?,
        CommunityAccountState::Deleted {
            completed_at_milliseconds: 1_700_000_000_000,
        }
    );

    database.shutdown()?;
    fixture.join()?;
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[tokio::test]
async fn application_report_rejects_a_receipt_for_another_operation() -> Result<(), Box<dyn Error>>
{
    let fixture = HttpFixture::start(
        vec![
            r#"{"operation_id":"00000000-0000-0000-0000-000000000001","id":"00000000-0000-0000-0000-000000000002","applied_revision":1,"revision":1,"status":"open"}"#,
        ],
        false,
    )?;
    let client = ChargingHttpClient::loopback(&fixture.origin).expect("loopback client");
    let report = ChargingContentReport::new(Uuid::new_v4(), Uuid::new_v4(), "Report details")
        .map_err(|_| io::Error::other("invalid report fixture"))?;
    let result = ChargingSyncService::submit_report(&client, &report, &"b".repeat(64)).await;
    match result {
        Err(cutout_application::charging::reports::ReportSubmitError::InvalidReceipt) => {}
        result => {
            return Err(io::Error::other(format!("unexpected report result: {result:?}")).into());
        }
    }
    assert!(
        fixture
            .next_request()
            .await?
            .starts_with("POST /v1/charging-spots/")
    );
    fixture.join()?;
    Ok(())
}

#[tokio::test]
async fn invalid_deletion_receipt_keeps_durable_fence_and_retry_settles_it()
-> Result<(), Box<dyn Error>> {
    let (directory, database) = open_test_database()?;
    let fixture = HttpFixture::start(
        vec![
            r#"{"status":"completed","completed_at_ms":0}"#,
            r#"{"status":"completed","completed_at_ms":1700000000001}"#,
        ],
        false,
    )?;
    let client = ChargingHttpClient::loopback(&fixture.origin).expect("loopback client");
    let service = ChargingSyncService::new(database.clone());

    match service
        .delete_community_account(&client, &"c".repeat(64))
        .await
    {
        Err(AccountDeletionError::Network) => {}
        result => {
            return Err(
                io::Error::other(format!("unexpected first deletion result: {result:?}")).into(),
            );
        }
    }
    assert_eq!(
        database.community_account_state()?,
        CommunityAccountState::Deleting
    );
    assert!(
        fixture
            .next_request()
            .await?
            .starts_with("DELETE /v1/identities/current ")
    );

    assert_eq!(
        service
            .delete_community_account(&client, &"c".repeat(64))
            .await?,
        1_700_000_000_001
    );
    assert!(
        fixture
            .next_request()
            .await?
            .starts_with("DELETE /v1/identities/current ")
    );
    assert_eq!(
        database.community_account_state()?,
        CommunityAccountState::Deleted {
            completed_at_milliseconds: 1_700_000_000_001,
        }
    );

    database.shutdown()?;
    let reopened = RideDatabase::open(&directory.join("rides.sqlite"))?;
    assert_eq!(
        reopened.community_account_state()?,
        CommunityAccountState::Deleted {
            completed_at_milliseconds: 1_700_000_000_001,
        }
    );
    assert_eq!(
        ChargingSyncService::new(reopened.clone())
            .delete_community_account(&client, &"c".repeat(64))
            .await?,
        1_700_000_000_001
    );
    assert!(
        fixture
            .requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .try_recv()
            .is_err()
    );
    reopened.shutdown()?;
    fixture.join()?;
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
