/// Dispatches tool calls to handler methods based on tool name
///
/// # Example
/// ```
/// tool_dispatch!(self, name, args, progress_sender, {
///     "run_command" => handle_run_command,
///     "list_directory" => handle_list_directory,
/// })
/// ```
#[macro_export]
macro_rules! tool_dispatch {
    ($self:ident, $name:expr, $args:expr, $sender:expr, {
        $($key:expr => $method:ident),* $(,)?
    }) => {
        match $name {
            $(
                $key => $self.$method($args, $sender).await,
            )*
            _ => Err($crate::MCPError::UnknownTool($name.into())),
        }
    };
}

/// Helper macro for extracting required parameters from JSON args
#[macro_export]
macro_rules! extract_required {
    // String
    ($args:expr, $key:expr, String) => {
        $args.get($key)
            .and_then(|v| v.as_str().map(|s| s.to_owned()))
            .ok_or($crate::MCPError::MissingParameters)?
    };
    // &str (returns String for convenience)
    ($args:expr, $key:expr, &str) => {
        $args.get($key)
            .and_then(|v| v.as_str().map(|s| s.to_owned()))
            .ok_or($crate::MCPError::MissingParameters)?
    };
    // i64
    ($args:expr, $key:expr, i64) => {
        $args.get($key)
            .and_then(|v| v.as_i64())
            .ok_or($crate::MCPError::MissingParameters)?
    };
    // u64
    ($args:expr, $key:expr, u64) => {
        $args.get($key)
            .and_then(|v| v.as_u64())
            .ok_or($crate::MCPError::MissingParameters)?
    };
    // f64
    ($args:expr, $key:expr, f64) => {
        $args.get($key)
            .and_then(|v| v.as_f64())
            .ok_or($crate::MCPError::MissingParameters)?
    };
    // bool
    ($args:expr, $key:expr, bool) => {
        $args.get($key)
            .and_then(|v| v.as_bool())
            .ok_or($crate::MCPError::MissingParameters)?
    };
}

/// Helper macro for extracting optional parameters with defaults
#[macro_export]
macro_rules! extract_optional {
    // String
    ($args:expr, $key:expr, String, $default:expr) => {
        $args.get($key)
            .and_then(|v| v.as_str().map(|s| s.to_owned()))
            .unwrap_or($default)
    };
    // &str (returns String for convenience)
    ($args:expr, $key:expr, &str, $default:expr) => {
        $args.get($key)
            .and_then(|v| v.as_str().map(|s| s.to_owned()))
            .unwrap_or($default)
    };
    // i64
    ($args:expr, $key:expr, i64, $default:expr) => {
        $args.get($key)
            .and_then(|v| v.as_i64())
            .unwrap_or($default)
    };
    // u64
    ($args:expr, $key:expr, u64, $default:expr) => {
        $args.get($key)
            .and_then(|v| v.as_u64())
            .unwrap_or($default)
    };
    // f64
    ($args:expr, $key:expr, f64, $default:expr) => {
        $args.get($key)
            .and_then(|v| v.as_f64())
            .unwrap_or($default)
    };
    // bool
    ($args:expr, $key:expr, bool, $default:expr) => {
        $args.get($key)
            .and_then(|v| v.as_bool())
            .unwrap_or($default)
    };
}
