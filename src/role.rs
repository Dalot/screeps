use serde::{Deserialize, Serialize};

use crate::creep::*;

#[derive(Debug, Serialize, Deserialize)]
pub enum Role {
    Harvester,
    Hauler,
}
