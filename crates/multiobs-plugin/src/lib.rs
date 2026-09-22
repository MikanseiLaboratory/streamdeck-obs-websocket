pub mod actions;
pub mod contracts;
pub mod kind;
pub mod ops;
pub mod render;
pub mod state;

pub fn force_link() {
    actions::force_link();
}
