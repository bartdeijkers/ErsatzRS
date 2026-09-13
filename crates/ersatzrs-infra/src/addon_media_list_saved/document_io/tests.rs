use std::io::Cursor;

use super::*;
use crate::addon_media_list_saved::model::tests::fixture;

#[test]
fn saved_document_io_round_trip_measures_text_not_external_media() {
    let mut document = fixture();
    // Referenced bytes are never read or included in the JSON-text quota.
    document.published.as_mut().unwrap().files[0].byte_length = 800 * 1024 * 1024 * 1024;
    let mut bytes = Vec::new();
    let size = write_document(&mut bytes, &document).unwrap();
    assert_eq!(size, bytes.len() as u64);
    assert_eq!(serialized_size(&document).unwrap(), size);
    let mut input = Cursor::new(bytes);
    input.set_position(12);
    assert_eq!(read_document(&mut input).unwrap(), document);
}

#[test]
fn saved_document_io_exact_limit_and_one_byte_over_leave_output_untouched() {
    let mut document = fixture();
    document.draft.presentation.name = "Épisode \"quoted\"".into();
    let bytes = serde_json::to_vec(&document).unwrap();
    let length = bytes.len() as u64;
    assert_eq!(size_with_limit(&document, length).unwrap(), length);
    assert_eq!(
        read_with_limit(&mut Cursor::new(&bytes), length).unwrap(),
        document
    );
    let mut output = Vec::new();
    assert_eq!(
        write_with_limit(&mut output, &document, length).unwrap(),
        length
    );
    assert_eq!(output, bytes);
    let before = output.clone();
    assert_eq!(
        write_with_limit(&mut output, &document, length - 1),
        Err(SavedDocumentError::TooLarge)
    );
    assert_eq!(output, before);
    assert_eq!(
        read_with_limit(&mut Cursor::new(&bytes), length - 1),
        Err(SavedDocumentError::TooLarge)
    );
    // Whitespace is also saved text, even though it adds no model fields.
    let mut padded = bytes;
    padded.push(b' ');
    assert_eq!(
        read_with_limit(&mut Cursor::new(padded), length),
        Err(SavedDocumentError::TooLarge)
    );
}

#[test]
fn saved_document_io_rejects_real_limit_before_reading_large_input() {
    assert_eq!(MAX_SAVED_DOCUMENT_BYTES, 268_435_456);
    // set_len creates a sparse fixture; do not allocate or parse 256 MiB in tests.
    let file = tempfile::tempfile().unwrap();
    file.set_len(MAX_SAVED_DOCUMENT_BYTES + 1).unwrap();
    let mut file = std::io::BufReader::new(file);
    assert_eq!(read_document(&mut file), Err(SavedDocumentError::TooLarge));
}

#[test]
fn saved_document_io_reports_future_version_without_requiring_current_fields() {
    let future = br#"{"future_only": {"anything": [1, 2]}, "version": 2}"#;
    assert_eq!(
        read_document(&mut Cursor::new(future)),
        Err(SavedDocumentError::UnsupportedVersion)
    );
    let mut document = fixture();
    document.version += 1;
    let mut output = Vec::new();
    assert_eq!(
        write_document(&mut output, &document),
        Err(SavedDocumentError::UnsupportedVersion)
    );
    assert!(output.is_empty());
    for invalid in [
        br#"{"version": 1}"#.as_slice(),
        br#"{"version": 1, "version": 1}"#,
        br#"{"version": 1} trailing"#,
        br#"{"draft": "fixture"}"#,
    ] {
        assert_eq!(
            read_document(&mut Cursor::new(invalid)),
            Err(SavedDocumentError::Invalid)
        );
    }
    let mut value = serde_json::to_value(fixture()).unwrap();
    value["publication_journal"] = serde_json::json!({"operation_id": 123});
    assert_eq!(
        read_document(&mut Cursor::new(serde_json::to_vec(&value).unwrap())),
        Err(SavedDocumentError::Invalid)
    );
}

#[test]
fn saved_document_io_caps_reads_when_input_grows_after_size_probe() {
    struct GrownInput(Cursor<Vec<u8>>);
    impl Read for GrownInput {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.0.read(buffer)
        }
    }
    impl Seek for GrownInput {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            if position == SeekFrom::End(0) {
                Ok(1) // Simulate growth after the initial length observation.
            } else {
                self.0.seek(position)
            }
        }
    }
    let mut input = GrownInput(Cursor::new(vec![b' '; 4096]));
    assert_eq!(
        read_with_limit(&mut input, 16),
        Err(SavedDocumentError::TooLarge)
    );
    assert_eq!(input.0.position(), 17);
}

#[test]
fn saved_document_io_keeps_io_errors_redacted() {
    struct BrokenOutput;
    impl Write for BrokenOutput {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("fixture detail that must not escape"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let error = write_document(&mut BrokenOutput, &fixture()).unwrap_err();
    assert_eq!(error, SavedDocumentError::Io);
    assert!(!error.to_string().contains("fixture detail"));
}

#[test]
fn saved_document_io_buffers_json_chunks_and_propagates_flush_failure() {
    #[derive(Default)]
    struct Output {
        bytes: Vec<u8>,
        writes: usize,
        fail_flush: bool,
    }
    impl Write for Output {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            self.writes += 1;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            if self.fail_flush {
                Err(io::Error::other("fixture flush detail"))
            } else {
                Ok(())
            }
        }
    }
    let document = fixture();
    let expected = serde_json::to_vec(&document).unwrap();
    let mut output = Output::default();
    write_document(&mut output, &document).unwrap();
    assert_eq!(output.bytes, expected);
    assert!(
        output.writes < 4,
        "small fixture must not write per JSON token"
    );
    let mut failed = Output {
        fail_flush: true,
        ..Output::default()
    };
    assert_eq!(
        write_document(&mut failed, &document),
        Err(SavedDocumentError::Io)
    );
}
