pub mod cli;
mod client;
mod command;
mod server;
mod switch;
mod util;

pub use command::{IpcCommand, IpcCommandSend, Reply};
pub use server::{IpcExt, IpcRegistry, IpcServer, serve};
pub use switch::Switch;
pub use util::socket_path;
