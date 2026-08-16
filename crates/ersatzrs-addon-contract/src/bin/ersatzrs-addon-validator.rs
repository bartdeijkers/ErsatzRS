use std::path::Path;
use std::process::ExitCode;

use ersatzrs_addon_contract::{validate_contract_document, ContractDocumentKind};

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
            let value = args
                .get(index + 1)
                .ok_or_else(|| "--kind requires addon, repository, or catalog".to_owned())?;
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
        let contents = std::fs::read_to_string(path)
            .map_err(|error| format!("{path}: failed to read document: {error}"))?;
        validate_contract_document(document_kind, &contents)
            .map_err(|error| format!("{path}: {error}"))?;
        println!("validated {path}");
    }
    Ok(())
}

fn parse_kind(value: &str) -> Result<ContractDocumentKind, String> {
    match value {
        "addon" => Ok(ContractDocumentKind::AddonManifest),
        "repository" => Ok(ContractDocumentKind::RepositoryManifest),
        "catalog" => Ok(ContractDocumentKind::RepositoryIndex),
        _ => Err(format!(
            "unsupported document kind `{value}`; expected addon, repository, or catalog"
        )),
    }
}

fn infer_kind(path: &Path) -> ContractDocumentKind {
    match path.file_name().and_then(|name| name.to_str()) {
        Some("repository.toml") => ContractDocumentKind::RepositoryManifest,
        Some(name)
            if Path::new(name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json")) =>
        {
            ContractDocumentKind::RepositoryIndex
        }
        _ => ContractDocumentKind::AddonManifest,
    }
}

fn print_help() {
    eprintln!("ersatzrs-addon-validator — validate public add-on contract documents");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("    ersatzrs-addon-validator [--kind addon|repository|catalog] <PATH>...");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_the_two_manifest_names_and_catalog_json() {
        assert_eq!(
            infer_kind(Path::new("addon.toml")),
            ContractDocumentKind::AddonManifest
        );
        assert_eq!(
            infer_kind(Path::new("repository.toml")),
            ContractDocumentKind::RepositoryManifest
        );
        assert_eq!(
            infer_kind(Path::new("index-v1.json")),
            ContractDocumentKind::RepositoryIndex
        );
    }
}
