use std::path::Path;
use std::process::ExitCode;

use ersatzrs_addon_contract::{validate_contract_document, ContractDocumentKind};

pub fn run(args: Vec<String>) -> ExitCode {
    match validate_args(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("validate-addon-manifest: {message}");
            print_help();
            ExitCode::FAILURE
        }
    }
}

fn validate_args(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        return Err("no contract documents supplied".to_owned());
    }
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        print_help();
        return Ok(());
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
    eprintln!("validate-addon-manifest — validate public add-on contract documents");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!(
        "    cargo xtask validate-addon-manifest [--kind addon|repository|catalog] <PATH>..."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_kind_is_inferred_from_the_standard_filename() {
        assert_eq!(
            infer_kind(Path::new("addons/example/addon.toml")),
            ContractDocumentKind::AddonManifest
        );
        assert_eq!(
            infer_kind(Path::new("repository.toml")),
            ContractDocumentKind::RepositoryManifest
        );
        assert_eq!(
            infer_kind(Path::new("dist/index-v1.json")),
            ContractDocumentKind::RepositoryIndex
        );
    }
}
