use std::io::IsTerminal;

/// Whether to use colored output (only when stdout is a terminal).
pub fn use_color() -> bool {
    std::io::stdout().is_terminal()
}

/// Output configuration for agent-friendly CLI design.
///
/// Supports TTY detection (auto-JSON when piped), quiet mode,
/// and structured JSON output for all commands including mutations.
#[derive(Clone, Copy)]
pub struct OutputConfig {
    pub json: bool,
    pub quiet: bool,
}

impl OutputConfig {
    pub fn new(json_flag: bool, quiet: bool) -> Self {
        let json = json_flag || !std::io::stdout().is_terminal();
        Self { json, quiet }
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
        if self.json {
            println!(
                "{}",
                serde_json::to_string_pretty(json_value).expect("failed to serialize JSON")
            );
        } else {
            self.print_message(human_message);
        }
    }
}

/// Exit codes for agent-friendly error handling.
/// Agents can branch on specific failure modes without parsing error text.
pub mod exit_codes {
    pub const SUCCESS: i32 = 0;
    pub const CONFIG_ERROR: i32 = 2;
    pub const AUTH_ERROR: i32 = 3;
    pub const NOT_FOUND: i32 = 4;
    pub const API_ERROR: i32 = 5;
    pub const GENERAL_ERROR: i32 = 1;
}

/// Map an error to a specific exit code by downcasting to ApiError.
pub fn exit_code_for_error(err: &(dyn std::error::Error + 'static)) -> i32 {
    if let Some(api_err) = err.downcast_ref::<crate::api::ApiError>() {
        match api_err {
            crate::api::ApiError::Auth(_) => exit_codes::AUTH_ERROR,
            crate::api::ApiError::NotFound(_) => exit_codes::NOT_FOUND,
            crate::api::ApiError::Api { .. } => exit_codes::API_ERROR,
            crate::api::ApiError::Http(_) | crate::api::ApiError::Other(_) => {
                exit_codes::GENERAL_ERROR
            }
        }
    } else {
        exit_codes::GENERAL_ERROR
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
}
