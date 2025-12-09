// Debug module for frontend debugging capabilities
// Provides console capture, remote debugging, and script injection

pub mod types;
pub mod server;
pub mod session;
pub mod injector;

pub use types::*;
pub use server::DebugServer;
pub use session::SessionManager;
pub use injector::ScriptInjector;
