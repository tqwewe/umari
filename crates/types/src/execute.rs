use serde::{Deserialize, Serialize};

// Note: CommandPayload and EmittedEvent are defined in umari-runtime

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ExecuteResponse {
    /// Event store position after command execution
    pub position: Option<u64>,
    /// Events emitted by the command
    pub events: Vec<EmittedEventInfo>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EmittedEventInfo {
    /// Event type identifier
    pub event_type: String,
    /// Domain ID tags for event categorization
    pub tags: Vec<String>,
}
