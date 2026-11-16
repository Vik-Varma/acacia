mod congestion;
mod recv;
mod send;

pub(crate) use recv::RecvState;
pub use recv::{RecvError, RecvLimits};
pub(crate) use send::SendQueue;

pub const CHANNELS: usize = 32;

#[cfg(test)]
mod tests;
