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

/// Creates a tuple of [`FoldRunner`]s from a list of rule functions or closures.
///
/// Each expression is automatically wrapped in [`runner()`], so rules are written
/// as plain functions or closures without any boilerplate.
///
/// # Example
///
/// ```rust,ignore
/// fn rules(input: &Self::Input) -> impl RuleSet {
///     rules!(must_be_open, min_balance(input.amount))
/// }
/// ```
#[macro_export]
macro_rules! rules {
    ($($rule:expr),* $(,)?) => {
        ($($crate::rules::runner($rule),)*)
    };
}

#[macro_export]
macro_rules! reject {
    ($s:literal, $($t:tt)*) => {{
        return Err($crate::error::CommandError::reject(format!($s, $($t)*)))
    }};
    ($e:expr) => {{
        return Err($crate::error::CommandError::reject($e.to_string()))
    }};
}
