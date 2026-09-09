//! Permission-reviewed external tool maintenance wire contract.

use serde::{Deserialize, Serialize};

use crate::{invalid, is_setting_key, validate_tool_version, ContractError};

/// External executable maintenance capability, introduced in manifest schema 6.
pub const CAPABILITY_TOOL_UPDATE_V1: &str = "tool.update.v1";
/// Entrypoint operation used for an explicit tool update.
pub const TOOL_UPDATE_OPERATION: &str = "update";
/// Maximum executable-setting keys requested by a package.
pub const MAX_TOOL_UPDATE_PERMISSIONS: usize = 32;
/// Maximum serialized maintenance request, including whitespace.
pub const MAX_TOOL_UPDATE_REQUEST_BYTES: usize = 1024;
/// Maximum serialized maintenance result, including whitespace.
pub const MAX_TOOL_UPDATE_OUTPUT_BYTES: usize = 2048;

/// Request for one declared executable setting; executable paths remain settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolUpdateRequest {
    /// Must equal `tool.update.v1`.
    pub schema: String,
    /// Declared, approved executable-setting key.
    pub tool_key: String,
}

impl ToolUpdateRequest {
    /// Validate the schema and bounded executable-setting identity.
    /// # Errors
    /// Rejects unknown schemas or invalid setting keys.
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_identity(&self.schema, &self.tool_key)
    }

    /// Parse a size-bounded request without including input in diagnostics.
    /// # Errors
    /// Rejects oversized, malformed, or invalid requests.
    pub fn from_json(bytes: &[u8]) -> Result<Self, ContractError> {
        let request: Self = parse_json(bytes, MAX_TOOL_UPDATE_REQUEST_BYTES)?;
        request.validate()?;
        Ok(request)
    }
}

/// Provider-reported outcome. Replacement validation and rollback are host work.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolUpdateOutcome {
    Updated,
    AlreadyCurrent,
    ManualUpdateRequired,
    Failed,
}

/// Stable host-localizable diagnostics; provider output prose is never forwarded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolUpdateDiagnosticCode {
    UnsupportedInstallation,
    UpdateFailed,
    VersionProbeFailed,
    ToolUnavailable,
}

/// Bounded version evidence from one explicit maintenance invocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolUpdateResult {
    /// Must equal `tool.update.v1`.
    pub schema: String,
    /// Must match the requested executable-setting key.
    pub tool_key: String,
    /// Normalized local version before invoking the updater, when available.
    pub before_version: Option<String>,
    /// Normalized local version after invoking the updater, when available.
    pub after_version: Option<String>,
    /// Typed provider outcome, independent of updater output wording.
    pub outcome: ToolUpdateOutcome,
    /// Optional allowlisted diagnostic suitable for host localization.
    pub diagnostic_code: Option<ToolUpdateDiagnosticCode>,
}

impl ToolUpdateResult {
    /// Validate identity and version evidence without accepting diagnostic prose.
    /// # Errors
    /// Rejects unknown schemas, invalid keys, or unsafe version identifiers.
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_identity(&self.schema, &self.tool_key)?;
        for version in [&self.before_version, &self.after_version]
            .into_iter()
            .flatten()
        {
            validate_tool_version(version)?;
        }
        Ok(())
    }

    /// Parse size-bounded provider output with redacted parse errors.
    /// # Errors
    /// Rejects oversized, malformed, or invalid results.
    pub fn from_json(bytes: &[u8]) -> Result<Self, ContractError> {
        let result: Self = parse_json(bytes, MAX_TOOL_UPDATE_OUTPUT_BYTES)?;
        result.validate()?;
        Ok(result)
    }
}

fn validate_identity(schema: &str, key: &str) -> Result<(), ContractError> {
    if schema != CAPABILITY_TOOL_UPDATE_V1 {
        return invalid("tool update schema", "unsupported schema");
    }
    if !is_setting_key(key) {
        return invalid("tool update tool_key", "requires an executable-setting key");
    }
    Ok(())
}

fn parse_json<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    maximum: usize,
) -> Result<T, ContractError> {
    if bytes.len() > maximum {
        return invalid("tool update payload", "exceeds the byte limit");
    }
    serde_json::from_slice(bytes).map_err(|_| ContractError::Invalid {
        field: "tool update payload",
        message: "invalid JSON contract".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result() -> ToolUpdateResult {
        ToolUpdateResult {
            schema: CAPABILITY_TOOL_UPDATE_V1.to_owned(),
            tool_key: "TOOL_BIN".to_owned(),
            before_version: Some("2026.09.01".to_owned()),
            after_version: Some("2026.09.08".to_owned()),
            outcome: ToolUpdateOutcome::Updated,
            diagnostic_code: None,
        }
    }

    #[test]
    fn round_trips_all_typed_outcomes() {
        for outcome in [
            ToolUpdateOutcome::Updated,
            ToolUpdateOutcome::AlreadyCurrent,
            ToolUpdateOutcome::ManualUpdateRequired,
            ToolUpdateOutcome::Failed,
        ] {
            let mut expected = result();
            expected.outcome = outcome;
            let bytes = serde_json::to_vec(&expected).unwrap();
            assert_eq!(ToolUpdateResult::from_json(&bytes).unwrap(), expected);
        }
    }

    #[test]
    fn rejects_prose_paths_unknown_diagnostics_and_oversized_payloads() {
        for unsafe_version in ["version with prose", "bin/tool", "v1\nprivate", ""] {
            let mut response = result();
            response.before_version = Some(unsafe_version.to_owned());
            assert!(response.validate().is_err());
        }
        let mut response = serde_json::to_value(result()).unwrap();
        response["diagnostic_code"] = serde_json::json!("private provider text");
        let error =
            ToolUpdateResult::from_json(&serde_json::to_vec(&response).unwrap()).unwrap_err();
        assert!(!error.to_string().contains("private"));
        assert!(
            ToolUpdateResult::from_json(&vec![b' '; MAX_TOOL_UPDATE_OUTPUT_BYTES + 1]).is_err()
        );
        let mut response = serde_json::to_value(result()).unwrap();
        response["stderr"] = serde_json::json!("private output");
        assert!(ToolUpdateResult::from_json(&serde_json::to_vec(&response).unwrap()).is_err());
    }

    #[test]
    fn request_rejects_unknown_schema_and_non_setting_identity() {
        let mut request = ToolUpdateRequest {
            schema: CAPABILITY_TOOL_UPDATE_V1.to_owned(),
            tool_key: "TOOL_BIN".to_owned(),
        };
        assert_eq!(
            ToolUpdateRequest::from_json(&serde_json::to_vec(&request).unwrap()).unwrap(),
            request
        );
        request.schema = "tool.update.v2".to_owned();
        assert!(request.validate().is_err());
        request.schema = CAPABILITY_TOOL_UPDATE_V1.to_owned();
        request.tool_key = "bin/tool".to_owned();
        assert!(request.validate().is_err());
        assert!(
            ToolUpdateRequest::from_json(&vec![b' '; MAX_TOOL_UPDATE_REQUEST_BYTES + 1]).is_err()
        );
    }
}
