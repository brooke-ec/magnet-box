use serde::Deserialize;
use serde_inline_default::serde_inline_default;

#[serde_inline_default]
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AppConfiguration {
    /// The interval in seconds that clients should wait between sending regular requests to the tracker.
    #[serde_inline_default(1800)]
    pub(crate) interval: u32,

    /// The default number of peers to return in a response to an announce request.
    #[serde_inline_default(50)]
    pub(crate) numwant: usize,
}
