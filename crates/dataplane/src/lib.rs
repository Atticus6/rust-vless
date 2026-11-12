//! Shared VLESS data plane used by both the master and child nodes.

pub mod event;
pub mod frame;
pub mod relay;
pub mod router;
pub mod session;
pub mod state;

pub use event::{AccessRecord, DataEvent, LinkTrafficRecord};
pub use relay::RelayService;
pub use router::router;
pub use session::{run_session, SessionContext, SessionMode};
pub use state::{DataPlaneConfig, DataPlaneState, Inbound, Outbound};
