//! XML 1.0 output boundary for both bundled and custom guide templates.

use std::{
    borrow::Cow,
    io::{Cursor, Write},
};

use quick_xml::{events::Event, Reader, Writer};

/// XML 1.0's Char production, including legal whitespace and supplementary
/// Unicode. Escaping cannot make a forbidden character legal in XML.
fn is_xml_char(value: char) -> bool {
    matches!(value, '\u{9}' | '\u{a}' | '\u{d}' | '\u{20}'..='\u{d7ff}'
        | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

/// Replace unrepresentable characters only at serialization time; keep stored
/// metadata and every legal Unicode character intact. Borrow the common case.
pub(super) fn sanitize_xml(value: &str) -> Cow<'_, str> {
    if value.chars().all(is_xml_char) {
        Cow::Borrowed(value)
    } else {
        Cow::Owned(
            value
                .chars()
                .map(|c| if is_xml_char(c) { c } else { '\u{fffd}' })
                .collect(),
        )
    }
}

/// Check XML syntax, references, names, attributes and namespaces. Keep errors
/// redacted: parser diagnostics can otherwise contain private metadata or URLs.
pub(super) fn validate_xml(value: &str) -> Result<(), quick_xml::Error> {
    roxmltree::Document::parse(value).map_err(|error| {
        let position = error.pos();
        quick_xml::Error::from(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid XML at {}:{}", position.row, position.col),
        ))
    })?;
    // roxmltree 0.21 replaces surrogate/out-of-range numeric references with
    // U+FFFD internally. We preserve serialized bytes, so reject those references
    // explicitly instead of accepting bytes that a strict client cannot parse.
    validate_character_references(value)
}

fn validate_character_references(value: &str) -> Result<(), quick_xml::Error> {
    let mut reader = Reader::from_str(value);
    loop {
        match reader.read_event()? {
            Event::Start(start) | Event::Empty(start) => {
                for attribute in start.attributes() {
                    attribute?.normalized_value(quick_xml::XmlVersion::Explicit1_0)?;
                }
            }
            Event::GeneralRef(reference) => {
                reference.resolve_char_ref()?;
            }
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}

/// Validate the entire fragment in its actual nested context before committing
/// any bytes. The wrapper rejects declarations/DTDs and unmatched outer tags;
/// roxmltree also rejects malformed entities/attributes that quick-xml's event
/// reader deliberately does not validate. No external entities are resolved.
pub(super) fn append_fragment(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    fragment: &str,
) -> Result<(), quick_xml::Error> {
    let fragment = sanitize_xml(fragment);
    validate_xml(&format!("<tv>{fragment}</tv>"))?;
    // Commit only validated bytes. Preserve whitespace around references,
    // comments and CDATA instead of trimming individual text chunks.
    writer.get_mut().write_all(fragment.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xmltv_rejects_malformed_fragments_without_partial_output() {
        for fragment in [
            "<programme><title>A & B</title></programme>",
            "<programme><title>&nbsp;</title></programme>",
            "<programme><title>&#0;</title></programme>",
            "<programme><title>&#xFFFE;</title></programme>",
            "<programme><title>&#xD800;</title></programme>",
            "<programme><title>&#x110000;</title></programme>",
            "<programme><title>]]></title></programme>",
            "<programme a=\"A & B\"/>",
            "<programme a=\"&#xFFFF;\"/>",
            "<programme a=\"&#xD800;\"/>",
            "<programme a=\"&#1114112;\"/>",
            "<programme a=\"1\" a=\"2\"/>",
            "<programme a=\"<\"/>",
            "<programme a=no-quotes/>",
            "<1programme/>",
            "<x:programme/>",
            "<programme><!-- invalid -- comment --></programme>",
            "<?xml version=\"1.0\"?><programme/>",
            "<!DOCTYPE programme SYSTEM 'https://example.test/epg.dtd'><programme/>",
            "</tv><tv><programme/>",
            "<programme><title>unclosed</title>",
        ] {
            let mut writer = Writer::new(Cursor::new(b"existing".to_vec()));
            assert!(
                append_fragment(&mut writer, fragment).is_err(),
                "{fragment}"
            );
            assert_eq!(writer.into_inner().into_inner(), b"existing", "{fragment}");
        }
    }

    #[test]
    fn xmltv_preserves_valid_entities_cdata_namespaces_and_unicode() {
        let fragment = "<programme xmlns:x=\"urn:example\" x:label=\"A &amp; B\"><title>café 日本語 😀 &lt; &gt; &apos; &quot; &#x9; &#10; &#13; &#x1F600;</title><desc><![CDATA[A & B < C]]></desc><!-- valid --></programme>";
        let mut writer = Writer::new(Cursor::new(Vec::new()));
        append_fragment(&mut writer, fragment).unwrap();
        let output = String::from_utf8(writer.into_inner().into_inner()).unwrap();
        let doc = roxmltree::Document::parse(&output).unwrap();
        assert_eq!(
            doc.root_element().attribute(("urn:example", "label")),
            Some("A & B")
        );
        assert_eq!(
            doc.descendants()
                .find(|n| n.has_tag_name("desc"))
                .unwrap()
                .text(),
            Some("A & B < C")
        );
        assert_eq!(
            doc.descendants()
                .find(|n| n.has_tag_name("title"))
                .unwrap()
                .text(),
            Some("café 日本語 😀 < > ' \" \t \n \r 😀"),
        );
    }

    #[test]
    fn xmltv_sanitizes_xml10_forbidden_characters_only() {
        let legal = "\t\n\r café 日本語 😀\u{7f}\u{85}\u{d7ff}\u{e000}\u{fffd}\u{10000}\u{10ffff}";
        assert!(matches!(sanitize_xml(legal), Cow::Borrowed(_)));
        assert_eq!(sanitize_xml(legal), legal);
        for c in (0..32)
            .filter_map(char::from_u32)
            .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
            .chain(['\u{fffe}', '\u{ffff}'])
        {
            assert_eq!(
                sanitize_xml(&format!("before{c}after")),
                "before\u{fffd}after"
            );
        }
    }
}
