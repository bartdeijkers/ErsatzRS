use std::path::Path;
use std::process::ExitCode;

use ersatzrs_addon_contract::{
    conform_media_list_output, validate_contract_document, ContractDocumentKind,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValidationKind {
    Document(ContractDocumentKind),
    MediaListOutput,
}

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    if args == ["--version"] {
        println!("ersatzrs-addon-validator {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.is_empty()
        || args
            .iter()
            .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        print_help();
        return if args.is_empty() {
            Err("no contract documents supplied".to_owned())
        } else {
            Ok(())
        };
    }

    let mut kind = None;
    let mut paths = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--kind" {
            let value = args.get(index + 1).ok_or_else(|| {
                "--kind requires addon, repository, catalog, or media-list-output".to_owned()
            })?;
            kind = Some(parse_kind(value)?);
            index += 2;
        } else if args[index].starts_with('-') {
            return Err(format!("unknown option `{}`", args[index]));
        } else {
            paths.push(args[index].as_str());
            index += 1;
        }
    }
    if paths.is_empty() {
        return Err("no contract documents supplied".to_owned());
    }

    for path in paths {
        let document_kind = kind.unwrap_or_else(|| infer_kind(Path::new(path)));
        let contents = std::fs::read(path)
            .map_err(|error| format!("{path}: failed to read document: {error}"))?;
        validate_input(document_kind, &contents).map_err(|error| format!("{path}: {error}"))?;
        println!("validated {path}");
    }
    Ok(())
}

fn validate_input(kind: ValidationKind, contents: &[u8]) -> Result<(), String> {
    match kind {
        ValidationKind::Document(kind) => {
            let text = std::str::from_utf8(contents)
                .map_err(|_| "contract document must be UTF-8".to_owned())?;
            validate_contract_document(kind, text).map_err(|error| error.to_string())
        }
        ValidationKind::MediaListOutput => conform_media_list_output(contents)
            .map(|_| ())
            .map_err(|error| error.to_string()),
    }
}

fn parse_kind(value: &str) -> Result<ValidationKind, String> {
    match value {
        "addon" => Ok(ValidationKind::Document(
            ContractDocumentKind::AddonManifest,
        )),
        "repository" => Ok(ValidationKind::Document(
            ContractDocumentKind::RepositoryManifest,
        )),
        "catalog" => Ok(ValidationKind::Document(
            ContractDocumentKind::RepositoryIndex,
        )),
        "media-list-output" => Ok(ValidationKind::MediaListOutput),
        _ => Err(format!(
            "unsupported document kind `{value}`; expected addon, repository, catalog, or media-list-output"
        )),
    }
}

fn infer_kind(path: &Path) -> ValidationKind {
    match path.file_name().and_then(|name| name.to_str()) {
        Some("repository.toml") => {
            ValidationKind::Document(ContractDocumentKind::RepositoryManifest)
        }
        Some(name)
            if Path::new(name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json")) =>
        {
            ValidationKind::Document(ContractDocumentKind::RepositoryIndex)
        }
        _ => ValidationKind::Document(ContractDocumentKind::AddonManifest),
    }
}

fn print_help() {
    eprintln!("ersatzrs-addon-validator — validate public add-on contracts and output");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!(
        "    ersatzrs-addon-validator [--kind addon|repository|catalog|media-list-output] <PATH>..."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_the_two_manifest_names_and_catalog_json() {
        assert_eq!(
            infer_kind(Path::new("addon.toml")),
            ValidationKind::Document(ContractDocumentKind::AddonManifest)
        );
        assert_eq!(
            infer_kind(Path::new("repository.toml")),
            ValidationKind::Document(ContractDocumentKind::RepositoryManifest)
        );
        assert_eq!(
            infer_kind(Path::new("index-v1.json")),
            ValidationKind::Document(ContractDocumentKind::RepositoryIndex)
        );
    }

    #[test]
    fn media_list_output_kind_uses_runtime_conformance() {
        let valid = br#"{"record_type":"list","provider_id":"fixture/list","name":"Fixture"}
{"record_type":"item","provider_id":"episode:1","rank":0,"display_title":"Episode","title":"Episode","kind":"remote_stream","source_url":"https://video.example.test/episode-1","metadata":{"release_date":"2026-08-22","artwork":[]}}
"#;
        validate_input(ValidationKind::MediaListOutput, valid).expect("valid media-list output");

        let invalid = br#"{"record_type":"list","provider_id":"fixture/list","name":"Fixture"}
{"record_type":"item","provider_id":"episode:1","rank":0,"display_title":"Episode","title":"Episode","kind":"remote_stream","source_url":"https://video.example.test/episode-1","metadata":{"release_date":"2026-08-22T12:00:00Z"}}
"#;
        let error = validate_input(ValidationKind::MediaListOutput, invalid)
            .expect_err("timestamp must not satisfy the Kodi date contract");
        assert!(error.contains("release_date"));
    }
}
