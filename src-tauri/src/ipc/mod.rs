//! Tauri IPC boundary (SPEC §1.1 layer 2, §3 `ipc/`): typed commands in, events out.
//! The frontend never talks to winget or the network; everything flows through here.

pub mod commands;
pub mod events;
pub mod state;

#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
