pub mod discover;
pub mod error;
pub mod reflect;
pub mod store;
pub mod teaching;
pub mod types;

use std::collections::HashMap;
use xuanji_agent::Agent;

pub use discover::DiscoverEngine;
pub use error::RoleError;
pub use reflect::LearningEngine;
pub use store::RoleStore;
pub use teaching::TeachingLibrary;
pub use types::*;
