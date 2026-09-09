use cutout_music::music_callback_path_matches;

#[test]
fn callback_accepts_both_url_root_path_representations() {
    for expected in ["", "/"] {
        for actual in ["", "/"] {
            assert!(
                music_callback_path_matches(expected, actual),
                "expected {expected:?}, actual {actual:?}"
            );
        }
    }
}

#[test]
fn callback_keeps_non_root_paths_exact() {
    assert!(music_callback_path_matches("/callback", "/callback"));
    for (expected, actual) in [
        ("", "/callback"),
        ("/callback", ""),
        ("/", "/callback"),
        ("/callback", "/"),
        ("/callback", "/callback/"),
        ("/callback/", "/callback"),
        ("/callback", "/callback/extra"),
        ("/callback", "/other"),
        ("/callback", "/Callback"),
        ("/", "//"),
    ] {
        assert!(
            !music_callback_path_matches(expected, actual),
            "expected {expected:?}, actual {actual:?}"
        );
    }
}
