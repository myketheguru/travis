//! Bluetooth LE — Travis service (v0.28.49 scaffold).
//!
//! This module declares the shape of Travis's BLE service so v0.28.50
//! can drop `btleplug` in without redesigning anything. Right now the
//! scan/advertise/file-transfer functions all return placeholder
//! values (empty peer list, "not yet" errors). The frontend can wire
//! the commands and future users of the crate will only need to fill
//! in the impls.
//!
//! # Service design
//!
//! Travis advertises a single GATT service under a static UUID:
//!
//!   Service:  550e8400-e29b-41d4-a716-446655440073  (Travis)
//!
//! With three characteristics:
//!
//!   Identity (read):
//!     UUID:  550e8400-e29b-41d4-a716-446655440074
//!     Value: JSON — { user_id, display_name, public_key }
//!     Purpose: advertise who we are so the other side can decide
//!     whether to pair. Public key is a Curve25519 identity key.
//!
//!   Handshake (write + notify):
//!     UUID:  550e8400-e29b-41d4-a716-446655440075
//!     Value in:  raw X25519 ephemeral public key from the peer
//!     Value out (notify): our X25519 ephemeral public key
//!     Purpose: derive a per-session symmetric key (X25519 → HKDF →
//!     ChaCha20-Poly1305) before any file bytes cross the link.
//!
//!   File chunks (write + notify):
//!     UUID:  550e8400-e29b-41d4-a716-446655440076
//!     Value: framed chunks — { seq: u32, len: u16, ciphertext, tag }
//!     Purpose: chunked encrypted file bytes. Chunks are
//!     ChaCha20-Poly1305 sealed with the session key + a nonce
//!     derived from seq. Receiver reassembles by seq; missing seqs
//!     are re-requested via a small control byte in the framing.
//!
//! The same session-key handshake + chunk framing is reused by the
//! T2T cloud-relayed transfer path, so a Travis can send a file to
//! another Travis regardless of which transport is available.
//!
//! # v0.28.49 vs v0.28.50
//!
//! v0.28.49 (this):
//!   - Module + types + Tauri commands wired
//!   - scan_peers() returns []
//!   - start_advertise() returns Ok(()) but is a no-op
//!   - send_file_ble() returns Err("not yet implemented in v0.28.50")
//!
//! v0.28.50:
//!   - Add btleplug + platform entitlements
//!   - Fill in scan + advertise using the Travis service UUID
//!   - Implement the handshake + chunk framing
//!   - Wire "Send file" affordance in ContactsOverlay

pub mod cmd;

use serde::{Deserialize, Serialize};

/// Static UUID for the Travis BLE service. See the module docs above
/// for the accompanying characteristic UUIDs.
pub const TRAVIS_SERVICE_UUID: &str = "550e8400-e29b-41d4-a716-446655440073";
pub const IDENTITY_CHAR_UUID: &str = "550e8400-e29b-41d4-a716-446655440074";
pub const HANDSHAKE_CHAR_UUID: &str = "550e8400-e29b-41d4-a716-446655440075";
pub const FILE_CHUNK_CHAR_UUID: &str = "550e8400-e29b-41d4-a716-446655440076";

/// A Travis discovered over BLE. Same shape as the mDNS
/// `DiscoveredPeer` so the frontend can render them in one radar
/// feed. `rssi` is the LE signal-strength dBm reading — negative
/// integer, closer to 0 means physically closer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlePeer {
    pub instance_id: String,
    pub display_name: Option<String>,
    pub user_id: Option<String>,
    pub public_key: Option<String>,
    pub rssi: Option<i32>,
    pub last_seen: i64,
}

/// Placeholder scan. Returns an empty list today. v0.28.50 replaces
/// this with a real btleplug adapter that browses for the Travis
/// service UUID and returns everyone in range.
pub fn scan_peers() -> Vec<BlePeer> {
    Vec::new()
}

/// Placeholder advertise. No-op today. v0.28.50 replaces this with a
/// btleplug adapter that registers the Travis GATT service +
/// broadcasts the identity characteristic.
pub fn start_advertise(
    _display_name: &str,
    _user_id: Option<&str>,
    _public_key: Option<&str>,
) -> Result<(), String> {
    Ok(())
}

/// Progress event emitted while a file is transferring. The frontend
/// subscribes to these to render the send/receive UI. Same envelope
/// works for BLE + T2T cloud transports.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    /// Client-side id for the transfer so the UI can track multiple
    /// in flight.
    pub transfer_id: String,
    pub transport: TransferTransport,
    /// Bytes moved so far.
    pub bytes: u64,
    /// Total bytes expected; None on inbound streams that don't
    /// advertise length up front.
    pub total_bytes: Option<u64>,
    pub state: TransferState,
    /// Set when state == Failed.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferTransport {
    Ble,
    T2t,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferState {
    /// Handshaking — deriving session key.
    Handshaking,
    /// Streaming chunks.
    Streaming,
    /// Transfer complete + integrity verified.
    Done,
    /// Failed with an error.
    Failed,
    /// User cancelled.
    Cancelled,
}

/// Placeholder for the future BLE file send. v0.28.50 replaces this
/// with the real handshake + chunk pipeline.
pub fn send_file_ble(
    _peer_instance_id: &str,
    _path: &str,
) -> Result<String, String> {
    Err("BLE file transfer will ship in v0.28.50 — the scaffold + \
         transport contract are in place. Use email invite or QR pair \
         to connect for now."
        .into())
}
