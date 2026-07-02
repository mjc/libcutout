use cutout_core::{
    LinkInfo, MonotonicTimestamp, ProtocolSession, SessionInput, SessionOutput, TransportAction,
    TransportWriteLimit,
};
use cutout_protocols::{BEGODE_FALCON_SESSION_KEY, find_session_registration};

fn main() -> Result<(), cutout_core::SessionOutputError> {
    let registration = find_session_registration(BEGODE_FALCON_SESSION_KEY)
        .expect("Falcon session registration should exist");
    let mut session = registration.construct();
    let mut output = Vec::<SessionOutput>::new();

    session.handle(
        SessionInput::LinkUp(LinkInfo {
            monotonic_ms: MonotonicTimestamp::new(1),
            max_write_len: Some(TransportWriteLimit::from_bytes(185)),
        }),
        &mut output,
    )?;

    assert!(output.iter().any(|item| matches!(
        item,
        SessionOutput::Transport(TransportAction::Subscribe { .. })
    )));
    println!(
        "read-only session produced {} transport action(s)",
        output.len()
    );
    Ok(())
}
