use std::io::IsTerminal;

/// Whether to use colored output (only when stdout is a terminal).
pub fn use_color() -> bool {
    std::io::stdout().is_terminal()
}

/// Output format selection.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Use JSON when stdout is not a terminal, text otherwise.
    Auto,
    /// Always output human-readable text.
    Text,
    /// Always output JSON.
    Json,
}

impl OutputFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(Self::Auto),
            "text" => Some(Self::Text),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

/// Output configuration for agent-friendly CLI design.
///
/// Supports TTY detection (auto-JSON when piped), quiet mode,
/// and structured JSON output for all commands including mutations.
#[derive(Clone, Copy)]
pub struct OutputConfig {
    pub format: OutputFormat,
    pub quiet: bool,
}

impl OutputConfig {
    pub fn new(format: OutputFormat, quiet: bool) -> Self {
        Self { format, quiet }
    }

    /// True when JSON output is active.
    pub fn is_json(&self) -> bool {
        match self.format {
            OutputFormat::Json => true,
            OutputFormat::Text => false,
            OutputFormat::Auto => !std::io::stdout().is_terminal(),
        }
    }

    /// Print data to stdout (tables or JSON). Always shown.
    pub fn print_data(&self, data: &str) {
        println!("{data}");
    }

    /// Print a human-readable message to stderr. Suppressed by --quiet.
    pub fn print_message(&self, msg: &str) {
        if !self.quiet {
            eprintln!("{msg}");
        }
    }

    /// Print a structured JSON result for mutation commands.
    /// In JSON mode, prints to stdout. In human mode, prints message to stderr.
    pub fn print_result(&self, json_value: &serde_json::Value, human_message: &str) {
        if self.is_json() {
            println!(
                "{}",
                serde_json::to_string_pretty(json_value).expect("failed to serialize JSON")
            );
        } else {
            self.print_message(human_message);
        }
    }
}

/// Write a structured error envelope as the last line of stderr.
/// Always call this before process::exit on non-zero paths.
pub fn print_error_envelope(kind: &str, message: &str, hint: Option<&str>) {
    let mut err = serde_json::json!({
        "kind": kind,
        "message": message,
    });
    if let Some(h) = hint {
        err["hint"] = serde_json::Value::String(h.to_string());
    }
    eprintln!(
        "{}",
        serde_json::to_string(&serde_json::json!({ "error": err }))
            .expect("failed to serialize error envelope")
    );
}

/// Exit codes for agent-friendly error handling.
/// Agents can branch on specific failure modes without parsing error text.
pub mod exit_codes {
    pub const SUCCESS: i32 = 0;
    pub const GENERAL_ERROR: i32 = 1;
    pub const CONFIG_ERROR: i32 = 2;
    pub const CONFIRMATION_REQUIRED: i32 = 2;
    pub const AUTH_ERROR: i32 = 3;
    pub const NOT_FOUND: i32 = 4;
    pub const API_ERROR: i32 = 5;
    pub const CONFLICT: i32 = 6;
}

/// Map an error to a specific exit code by downcasting to ApiError.
pub fn exit_code_for_error(err: &(dyn std::error::Error + 'static)) -> i32 {
    if let Some(api_err) = err.downcast_ref::<crate::api::ApiError>() {
        match api_err {
            crate::api::ApiError::Auth(_) => exit_codes::AUTH_ERROR,
            crate::api::ApiError::NotFound(_) => exit_codes::NOT_FOUND,
            crate::api::ApiError::Api { .. } => exit_codes::API_ERROR,
            crate::api::ApiError::Conflict(_) => exit_codes::CONFLICT,
            crate::api::ApiError::Http(_) | crate::api::ApiError::Other(_) => {
                exit_codes::GENERAL_ERROR
            }
        }
    } else {
        exit_codes::GENERAL_ERROR
    }
}

/// Map an error to its kind string and exit code.
pub fn error_kind_and_code(err: &(dyn std::error::Error + 'static)) -> (&'static str, i32) {
    if let Some(api_err) = err.downcast_ref::<crate::api::ApiError>() {
        match api_err {
            crate::api::ApiError::Auth(_) => ("auth_error", exit_codes::AUTH_ERROR),
            crate::api::ApiError::NotFound(_) => ("not_found", exit_codes::NOT_FOUND),
            // 408 and 429 are the two 4xx that invite the same request again,
            // so they stay retryable even though they are client errors.
            crate::api::ApiError::Api {
                status: 408 | 429, ..
            } => ("retry_later", exit_codes::API_ERROR),
            // Any other 4xx means the request itself was rejected, so retrying
            // it unchanged cannot help. It shares exit code 5 with api_error
            // but reports a distinct kind, so an agent branching on
            // `retryable` does not loop on a permanent failure.
            crate::api::ApiError::Api { status, .. } if (400..500).contains(status) => {
                ("client_error", exit_codes::API_ERROR)
            }
            crate::api::ApiError::Api { .. } => ("api_error", exit_codes::API_ERROR),
            crate::api::ApiError::Conflict(_) => ("conflict", exit_codes::CONFLICT),
            crate::api::ApiError::Http(_) | crate::api::ApiError::Other(_) => {
                ("general_error", exit_codes::GENERAL_ERROR)
            }
        }
    } else {
        ("general_error", exit_codes::GENERAL_ERROR)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ApiError;

    #[test]
    fn exit_code_for_auth_error() {
        let err = ApiError::Auth("bad key".into());
        assert_eq!(exit_code_for_error(&err), exit_codes::AUTH_ERROR);
    }

    #[test]
    fn exit_code_for_not_found() {
        let err = ApiError::NotFound("Client with MAC aa:bb".into());
        assert_eq!(exit_code_for_error(&err), exit_codes::NOT_FOUND);
    }

    #[test]
    fn exit_code_for_api_error() {
        let err = ApiError::Api {
            status: 500,
            message: "Internal Server Error".into(),
        };
        assert_eq!(exit_code_for_error(&err), exit_codes::API_ERROR);
    }

    #[test]
    fn exit_code_for_other_error() {
        let err = ApiError::Other("something".into());
        assert_eq!(exit_code_for_error(&err), exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn exit_code_for_non_api_error() {
        let err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        assert_eq!(exit_code_for_error(&err), exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn output_format_explicit_text_is_not_json() {
        let out = OutputConfig::new(OutputFormat::Text, false);
        assert!(!out.is_json());
    }

    #[test]
    fn output_format_explicit_json_is_json() {
        let out = OutputConfig::new(OutputFormat::Json, false);
        assert!(out.is_json());
    }

    #[test]
    fn error_kind_and_code_auth() {
        let err = ApiError::Auth("bad".into());
        let (kind, code) = error_kind_and_code(&err);
        assert_eq!(kind, "auth_error");
        assert_eq!(code, exit_codes::AUTH_ERROR);
    }

    #[test]
    fn error_kind_and_code_not_found() {
        let err = ApiError::NotFound("x".into());
        let (kind, code) = error_kind_and_code(&err);
        assert_eq!(kind, "not_found");
        assert_eq!(code, exit_codes::NOT_FOUND);
    }

    #[test]
    fn error_kind_and_code_client_error_for_a_rejected_request() {
        // The controller answers `power-cycle` on a PoE-disabled port with
        // HTTP 400 api.err.InvalidTargetPort. Retrying that unchanged can only
        // fail again, so it must not be published as the retryable api_error.
        let err = ApiError::Api {
            status: 400,
            message: "api.err.InvalidTargetPort".into(),
        };
        let (kind, code) = error_kind_and_code(&err);
        assert_eq!(kind, "client_error");
        assert_eq!(code, exit_codes::API_ERROR);
    }

    #[test]
    fn error_kind_and_code_keeps_408_and_429_retryable() {
        // Both statuses ask for the same request again, so they must not land
        // in the permanent client_error bucket an agent gives up on.
        for status in [408u16, 429] {
            let err = ApiError::Api {
                status,
                message: "slow down".into(),
            };
            let (kind, code) = error_kind_and_code(&err);
            assert_eq!(kind, "retry_later", "status {status}");
            assert_eq!(code, exit_codes::API_ERROR, "status {status}");
        }
    }

    #[test]
    fn error_kind_and_code_api_error_stays_for_server_side_failures() {
        for status in [500u16, 502, 503] {
            let err = ApiError::Api {
                status,
                message: "upstream failure".into(),
            };
            let (kind, code) = error_kind_and_code(&err);
            assert_eq!(kind, "api_error", "status {status}");
            assert_eq!(code, exit_codes::API_ERROR, "status {status}");
        }
    }

    #[test]
    fn error_envelope_is_valid_json() {
        let envelope = serde_json::json!({
            "error": {
                "kind": "auth_error",
                "message": "Authentication error: bad key",
            }
        });
        assert!(envelope["error"]["kind"].as_str().is_some());
        assert!(envelope["error"]["message"].as_str().is_some());
    }

    #[test]
    fn exit_code_for_conflict() {
        let err = ApiError::Conflict("port has no PoE".into());
        assert_eq!(exit_code_for_error(&err), exit_codes::CONFLICT);
    }

    #[test]
    fn error_kind_and_code_conflict() {
        let err = ApiError::Conflict("port has no PoE".into());
        let (kind, code) = error_kind_and_code(&err);
        assert_eq!(kind, "conflict");
        assert_eq!(code, 6);
    }
}
