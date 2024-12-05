pub mod api;
pub mod tree;
pub mod util;

use crate::tree::HashMapSearchTree;

pub struct AppState {
    pub tree: HashMapSearchTree,
}

#[cfg(feature = "gui")]
pub mod gui;
