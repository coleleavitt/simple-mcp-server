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
