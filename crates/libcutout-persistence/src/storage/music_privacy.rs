use super::StorageError;
use rusqlite::{Connection, OptionalExtension, params};

pub(super) fn delete_spotify_music_history(
    connection: &mut Connection,
) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    // Work one bounded ride timeline at a time rather than loading an unbounded ride list.
    while let Some(ride) = transaction
        .query_row(
            "SELECT ride_id FROM ride_music_event WHERE provider = 'spotify' LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        transaction.execute(
            "DELETE FROM ride_music_event WHERE ride_id = ?1 AND provider = 'spotify'",
            [&ride],
        )?;
        let sequences = transaction.prepare(
            "SELECT sequence FROM ride_music_event WHERE ride_id = ?1 ORDER BY sequence LIMIT ?2",
        )?.query_map(params![ride, cutout_music::MAX_MUSIC_TIMELINE_EVENTS + 1],
            |row| row.get::<_, i64>(0))?.collect::<Result<Vec<_>, _>>()?;
        if sequences.len() > cutout_music::MAX_MUSIC_TIMELINE_EVENTS {
            return Err(StorageError::MusicTimelineFull);
        }
        // Moving ascending rows down fills only gaps left by deleted Spotify rows.
        for (index, previous) in sequences.into_iter().enumerate() {
            let sequence = music_sequence(index)?;
            if previous != sequence {
                transaction.execute(
                    "UPDATE ride_music_event SET sequence = ?1 WHERE ride_id = ?2 AND sequence = ?3",
                    params![sequence, ride, previous],
                )?;
            }
        }
        transaction.execute(
            "UPDATE ride_music_history SET last_observed_at_ms =
             (SELECT MAX(observed_at_ms) FROM ride_music_event WHERE ride_id = ?1)
             WHERE ride_id = ?1",
            [&ride],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

fn music_sequence(index: usize) -> Result<i64, StorageError> {
    i64::try_from(index).map_err(|_| StorageError::MusicTimelineFull)
}

#[cfg(test)]
mod tests {
    use super::super::{
        configure_connection, create_started_live_ride, find_ride, music_events, save_music_event,
    };
    use super::*;
    use cutout_music::{
        MusicEventTiming, MusicHistoryPolicy, MusicProvider, MusicRideEvent, MusicRideEventKind,
    };

    #[test]
    fn spotify_disconnect_preserves_other_provider_events_and_rides() {
        let directory = tempfile::tempdir().expect("directory");
        let path = directory.path().join("music.sqlite");
        let mut connection = Connection::open(&path).expect("connection");
        configure_connection(&mut connection, &path).expect("schema");
        let ride = create_started_live_ride(
            &mut connection,
            1_700_000_000_000,
            100,
            None,
            Some(MusicHistoryPolicy::HumanReadable),
        )
        .expect("ride")
        .ride_id();
        let event = |provider, identifier: &str, at| {
            MusicRideEvent::new(
                provider,
                Some(identifier.into()),
                Some(identifier.into()),
                None,
                MusicRideEventKind::ItemChanged,
                MusicEventTiming {
                    observed_at: Some(cutout_core::MonotonicTimestamp::new(at)),
                    monotonic_at: cutout_core::MonotonicTimestamp::new(at),
                    wall_clock_at: cutout_core::WallClockUnixTimestamp::new(1_700_000_000_000 + at),
                    clock_uncertainty_milliseconds: 1,
                },
            )
            .expect("event")
        };
        let apple = event(MusicProvider::AppleMusic, "apple-item", 120);
        for (sequence, value) in [
            event(MusicProvider::Spotify, "spotify-item", 110),
            apple.clone(),
            event(MusicProvider::Spotify, "other-spotify-item", 130),
        ]
        .iter()
        .enumerate()
        {
            save_music_event(
                &mut connection,
                ride,
                MusicHistoryPolicy::HumanReadable,
                i64::try_from(sequence).expect("small index"),
                value,
            )
            .expect("stored");
        }
        delete_spotify_music_history(&mut connection).expect("disconnect cleanup");
        assert_eq!(
            music_events(&connection, ride).expect("history"),
            vec![apple]
        );
        assert!(find_ride(&connection, ride).expect("ride read").is_some());
        delete_spotify_music_history(&mut connection).expect("idempotent retry");
        let next = event(MusicProvider::AppleMusic, "next-apple-item", 140);
        save_music_event(
            &mut connection,
            ride,
            MusicHistoryPolicy::HumanReadable,
            1,
            &next,
        )
        .expect("remaining history continues with contiguous sequence");
        assert_eq!(music_events(&connection, ride).expect("history").len(), 2);
        drop(connection);
        let connection = Connection::open(&path).expect("reopen");
        assert!(
            music_events(&connection, ride)
                .expect("durable history")
                .iter()
                .all(|event| event.provider() == MusicProvider::AppleMusic)
        );
    }
}
