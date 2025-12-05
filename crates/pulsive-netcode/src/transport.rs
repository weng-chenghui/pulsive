//! Transport traits for network communication
//!
//! These traits define the interface for network communication.
//! Users implement these for their chosen network stack (UDP, WebSocket, etc.).

use std::net::SocketAddr;

/// Network address type
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Address {
    /// Socket address (IP + port)
    Socket(SocketAddr),
    /// Custom address (for WebSocket, WebRTC, etc.)
    Custom(String),
}

impl From<SocketAddr> for Address {
    fn from(addr: SocketAddr) -> Self {
        Address::Socket(addr)
    }
}

impl From<String> for Address {
    fn from(addr: String) -> Self {
        Address::Custom(addr)
    }
}

impl From<&str> for Address {
    fn from(addr: &str) -> Self {
        Address::Custom(addr.to_string())
    }
}

/// Connectionless transport trait (e.g., UDP)
///
/// Used for sending individual packets without connection state.
pub trait Transport: Send + Sync {
    /// Error type for this transport
    type Error: std::error::Error + Send + Sync + 'static;

    /// Send data to a target address
    fn send(&self, data: &[u8], target: &Address) -> Result<(), Self::Error>;

    /// Receive data (non-blocking)
    ///
    /// Returns `Ok(None)` if no data is available.
    /// Returns `Ok(Some((data, source)))` if data was received.
    fn recv(&self) -> Result<Option<(Vec<u8>, Address)>, Self::Error>;

    /// Get the local address this transport is bound to
    fn local_addr(&self) -> Option<Address>;
}

/// Connection-oriented transport trait (e.g., TCP, WebSocket)
///
/// Used for reliable, ordered communication with a specific peer.
pub trait Connection: Send + Sync {
    /// Error type for this connection
    type Error: std::error::Error + Send + Sync + 'static;

    /// Send data reliably (guaranteed delivery, ordered)
    fn send_reliable(&self, data: &[u8]) -> Result<(), Self::Error>;

    /// Send data unreliably (best effort, may be reordered or lost)
    ///
    /// For transports that don't support unreliable sends, this falls back to reliable.
    fn send_unreliable(&self, data: &[u8]) -> Result<(), Self::Error>;

    /// Receive data (non-blocking)
    ///
    /// Returns `Ok(None)` if no data is available.
    fn recv(&self) -> Result<Option<Vec<u8>>, Self::Error>;

    /// Check if the connection is still alive
    fn is_connected(&self) -> bool;

    /// Get the remote address
    fn remote_addr(&self) -> Option<Address>;

    /// Close the connection gracefully
    fn close(&self) -> Result<(), Self::Error>;
}

/// Packet types for the netcode protocol
#[allow(dead_code)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum PacketType {
    /// Input from client to server
    Input {
        /// Client tick when input was generated
        tick: u64,
        /// Serialized input data
        data: Vec<u8>,
    },
    /// State snapshot from server to client
    StateSnapshot {
        /// Server tick for this state
        tick: u64,
        /// Serialized model state
        data: Vec<u8>,
    },
    /// State delta from server to client (compressed)
    StateDelta {
        /// Base tick (client should have this state)
        base_tick: u64,
        /// Target tick after applying delta
        target_tick: u64,
        /// Delta data
        data: Vec<u8>,
    },
    /// Acknowledgment
    Ack {
        /// Tick being acknowledged
        tick: u64,
    },
    /// Ping for latency measurement
    Ping {
        /// Timestamp when ping was sent
        timestamp: u64,
    },
    /// Pong response
    Pong {
        /// Original ping timestamp
        timestamp: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_from() {
        let socket: Address = "127.0.0.1:8080".parse::<SocketAddr>().unwrap().into();
        assert!(matches!(socket, Address::Socket(_)));

        let custom: Address = "ws://localhost:8080".into();
        assert!(matches!(custom, Address::Custom(_)));
    }
}
