/// Convenience macro for emitting multiple events.
///
/// # Example
///
/// ```rust,ignore
/// fn handle(self, input: Input) -> Result<Emit, CommandError> {
///     Ok(emit![
///         SentFunds {
///             account_id: input.source_account,
///             amount: input.amount,
///             recipient_id: Some(input.dest_account.clone()),
///         },
///         ReceivedFunds {
///             account_id: input.dest_account,
///             amount: input.amount,
///             sender_id: Some(input.source_account),
///         },
///     ])
/// }
/// ```
///
/// Expands to:
///
/// ```rust,ignore
/// Emit::new()
///     .event(SentFunds { ... })
///     .event(ReceivedFunds { ... })
/// ```
#[macro_export]
macro_rules! emit {
    () => {
        $crate::emit::Emit::new()
    };
    ($($event:expr),+ $(,)?) => {
        $crate::emit::Emit::new()
            $(.event($event))+
    };
}
