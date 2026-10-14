//! Modular domain-driven services for BaileysClient.

pub mod commerce;
pub mod companion;
pub mod groups;
pub mod messages;
pub mod newsletter;
pub mod privacy;
pub mod status;

pub use commerce::*;
pub use companion::*;
pub use groups::*;
pub use messages::*;
pub use newsletter::*;
pub use privacy::*;
pub use status::*;
