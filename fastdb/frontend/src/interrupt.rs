//! A cancellation-only handle that does not keep a database connection alive.
use crate::Connection;
use std::sync::{Arc, Weak};
#[derive(Clone)]
pub struct InterruptHandle {
    connection: Weak<turso_core::Connection>,
}
impl InterruptHandle {
    /// Request interruption of active engine statements. Returns false if the
    /// connection has been dropped. True means live, not confirmed cancellation.
    /// Idle requests do not poison subsequent statements.
    pub fn interrupt(&self) -> bool {
        let Some(connection) = self.connection.upgrade() else {
            return false;
        };
        connection.interrupt();
        true
    }
}
impl Connection {
    pub fn interrupt_handle(&self) -> InterruptHandle {
        InterruptHandle {
            connection: Arc::downgrade(&self.engine),
        }
    }
}
