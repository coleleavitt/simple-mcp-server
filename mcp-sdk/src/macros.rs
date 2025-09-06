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
    ($args:expr, $key:expr, $type:ty) => {
        $args.get($key)
            .and_then(|v| <$type>::try_from(v).ok())
            .ok_or($crate::MCPError::MissingParameters)?
    };
}

/// Helper macro for extracting optional parameters with defaults
#[macro_export]
macro_rules! extract_optional {
    ($args:expr, $key:expr, $type:ty, $default:expr) => {
        $args.get($key)
            .and_then(|v| <$type>::try_from(v).ok())
            .unwrap_or($default)
    };
}
