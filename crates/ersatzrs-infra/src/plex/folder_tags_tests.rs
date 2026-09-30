use super::from_path;

#[test]
fn server_paths_include_library_and_subfolders_on_every_host() {
    for (file, root) in [
        (r"C:\media\other\Travel\Summer\clip.mkv", r"C:\media\other"),
        ("/media/other/Travel/Summer/clip.mkv", "/media/other"),
        (
            r"\\server\media\other\Travel\Summer\clip.mkv",
            r"\\server\media\other",
        ),
        (r"C:\media\other/Travel\Summer/clip.mkv", "C:/media/other/"),
        ("/media/other/Travel/Summer/clip.mkv", "/media/other/"),
        (r"C:\media\other\Travel\Summer\clip.mkv", r"c:\MEDIA\OTHER\"),
    ] {
        assert_eq!(
            from_path(file, &[root.into()]),
            ["other", "Travel", "Summer"],
            "{file}"
        );
    }
}

#[test]
fn roots_and_folder_boundaries_exclude_drives_and_filenames() {
    for (file, root, expected) in [
        ("/media/other/clip.mkv", "/media/other", vec!["other"]),
        (r"C:\media\other\clip.mkv", r"C:\media\other", vec!["other"]),
        ("/Travel/clip.mkv", "/", vec!["Travel"]),
        (r"C:\Travel\clip.mkv", r"C:\", vec!["Travel"]),
        ("/clip.mkv", "/", vec![]),
        (r"C:\clip.mkv", r"C:\", vec![]),
        ("/media/other-extra/clip.mkv", "/media/other", vec![]),
        (r"C:\media\other-extra\clip.mkv", r"C:\media\other", vec![]),
        ("clip.mkv", "/media/other", vec![]),
        ("", "/media/other", vec![]),
        (" ", "/media/other", vec![]),
        ("/media/Été/clip.mkv", "/MEDIA/ÉTÉ", vec!["Été"]),
        ("/media/Été/clip.mkv", "/MEDIA/ETE", vec![]),
        ("/media/Straße/clip.mkv", "/media/STRASSE", vec![]),
    ] {
        assert_eq!(from_path(file, &[root.into()]), expected, "{file}");
    }
}

#[test]
fn first_matching_root_controls_tags() {
    let roots = [
        "",
        "/media/other",
        "/media/other-extra",
        "/media/other-extra/Travel",
    ]
    .map(str::to_owned);
    assert_eq!(
        from_path("/media/other-extra/Travel/clip.mkv", &roots),
        ["other-extra", "Travel"]
    );
}
