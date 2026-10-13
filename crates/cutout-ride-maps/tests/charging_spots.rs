use cutout_ride_maps::{
    AccessNote, AccessNoteError, ChargingCost, ChargingVote, Coordinate, NewChargingSpot,
};

#[test]
fn free_and_paid_spots_accept_only_the_three_required_fields() {
    let coordinate = Coordinate::from_degrees(40.0, -105.0).expect("valid pin");

    for cost in [ChargingCost::Free, ChargingCost::Paid] {
        let spot = NewChargingSpot::new(coordinate, "  Outlet by the patio.  ", cost)
            .expect("minimum spot should be valid without optional metadata");
        assert_eq!(spot.coordinate(), coordinate);
        assert_eq!(spot.access_note().as_str(), "Outlet by the patio.");
        assert_eq!(spot.cost(), cost);
    }
}

#[test]
fn blank_access_notes_cannot_be_saved() {
    let coordinate = Coordinate::from_degrees(40.0, -105.0).expect("valid pin");
    for note in ["", " \n\t\r ", "\u{2003}\u{00a0}"] {
        assert_eq!(
            NewChargingSpot::new(coordinate, note, ChargingCost::Free),
            Err(AccessNoteError::Blank)
        );
    }
}

#[test]
fn access_notes_have_a_utf8_byte_bound() {
    assert!(AccessNote::new(&"a".repeat(2_000)).is_ok());
    assert_eq!(
        AccessNote::new(&"a".repeat(2_001)),
        Err(AccessNoteError::TooLong)
    );
    assert!(AccessNote::new(&"é".repeat(1_000)).is_ok());
    assert_eq!(
        AccessNote::new(&"é".repeat(1_001)),
        Err(AccessNoteError::TooLong)
    );
    assert_eq!(
        AccessNote::new(&format!(" \n{}\t ", "a".repeat(2_000)))
            .expect("surrounding whitespace is normalized")
            .as_str()
            .len(),
        2_000
    );
}

#[test]
fn access_notes_reject_embedded_controls_but_keep_multiline_instructions() {
    for note in [
        "Outlet\0outside",
        "Outlet\u{001b}outside",
        "Outlet\u{007f}outside",
        "Outlet\u{0085}outside",
    ] {
        assert_eq!(
            AccessNote::new(note),
            Err(AccessNoteError::ControlCharacter)
        );
    }
    let note = "Outside by the patio.\r\nAsk staff:\tkey needed.";
    assert_eq!(
        AccessNote::new(note)
            .expect("multiline instructions")
            .as_str(),
        note
    );
}

#[test]
fn pin_vote_selection_sets_replaces_or_clears_the_requested_vote() {
    for (current, selected, expected) in [
        (None, ChargingVote::Up, Some(ChargingVote::Up)),
        (None, ChargingVote::Down, Some(ChargingVote::Down)),
        (Some(ChargingVote::Up), ChargingVote::Up, None),
        (Some(ChargingVote::Down), ChargingVote::Down, None),
        (
            Some(ChargingVote::Up),
            ChargingVote::Down,
            Some(ChargingVote::Down),
        ),
        (
            Some(ChargingVote::Down),
            ChargingVote::Up,
            Some(ChargingVote::Up),
        ),
    ] {
        assert_eq!(selected.select(current), expected);
    }
}
