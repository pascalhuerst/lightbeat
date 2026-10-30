//! The groups and objects as the UI last saw them, shared with the effect
//! stacks so they can work out which strips and fixtures their groups hold.

use std::sync::{Arc, Mutex};

use crate::objects::group::Group;
use crate::objects::object::Object;

pub struct GroupContext {
    pub groups: Vec<Group>,
    pub objects: Vec<Object>,
}

pub type SharedGroupContext = Arc<Mutex<GroupContext>>;

pub fn new_shared_group_context() -> SharedGroupContext {
    Arc::new(Mutex::new(GroupContext { groups: Vec::new(), objects: Vec::new() }))
}
