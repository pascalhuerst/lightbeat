//! The format history of the setup file (`setup.json`, exported setups).
//!
//! 1: the first format with a version; files written before the field existed
//!    are the same and count as 1.

use super::{no_nested, Schema};

pub const SCHEMA: Schema = Schema { name: "setup", baseline: 1, steps: &[], nested: no_nested };
