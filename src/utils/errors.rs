use quasar_lang::prelude::*;

#[error_code]
pub enum DappError {
    /// Tree is full - not more leaves can be added
    TreeIsFull,
}