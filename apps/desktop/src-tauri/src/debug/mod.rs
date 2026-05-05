// Debug module for frontend debugging capabilities
// Provides console capture, remote debugging, and script injection

pub mod injector;
pub mod server;
pub mod session;
pub mod types;

pub use injector::ScriptInjector;
pub use server::DebugServer;
pub use session::SessionManager;
pub use types::*;
