//! Bluetooth LE — Travis service (v0.28.51 — real btleplug scan).
//!
//! Central-role (scan) is real and cross-platform via btleplug.
//! Peripheral-role (advertise) is not covered by btleplug across all
//! three OSes; the v0.28.52 pass pairs a peripheral crate on top of
//! this so peer discovery becomes bidirectional. Until then, a
//! Travis running v0.28.51 can *see* peers that advertise the
//! Travis service (e.g. same-network Travises using a companion
//! script) but can't be seen back over BLE.
//!
//! # Service design
//!
//! Travis's GATT service:
//!   Service:  550e8400-e29b-41d4-a716-446655440073  (Travis)
//!
//! Characteristics (v0.28.52+ read/write):
//!   Identity (read):     550e8400-e29b-41d4-a716-446655440074
//!     JSON: { user_id, display_name, public_key }
//!   Handshake (write + notify): 550e8400-e29b-41d4-a716-446655440075
//!     X25519 ephemeral public keys either direction → HKDF →
//!     ChaCha20-Poly1305 session key
//!   File chunk (write + notify): 550e8400-e29b-41d4-a716-446655440076
//!     Framed { seq, len, ciphertext, tag } — same framing used by
//!     the T2T cloud-relayed transfer path
//!
//! # Runtime model
//!
//! `ensure_scanner()` starts a tokio task the first time it's
//! called. The task:
//!   1. Grabs the default BLE adapter via btleplug::Manager
//!   2. Starts a scan filtered to the Travis service UUID
//!   3. Every ~1s reads discovered peripherals, extracts anything
//!      that looks like Travis (name, advertised UUID, TX power)
//!      and updates a shared HashMap
//! Anything that fails (no adapter, permission denied, etc.) is
//! logged but not surfaced as an error so the frontend keeps
//! working with an empty peer list.

pub mod cmd;

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use btleplug::api::{
    Central, CentralEvent, Manager as _, Peripheral as _, ScanFilter,
};
use btleplug::platform::Manager;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, OnceCell, RwLock};
use uuid::Uuid;

pub const TRAVIS_SERVICE_UUID: &str = "550e8400-e29b-41d4-a716-446655440073";
pub const IDENTITY_CHAR_UUID: &str = "550e8400-e29b-41d4-a716-446655440074";
pub const HANDSHAKE_CHAR_UUID: &str = "550e8400-e29b-41d4-a716-446655440075";
pub const FILE_CHUNK_CHAR_UUID: &str = "550e8400-e29b-41d4-a716-446655440076";

/// A Travis discovered over BLE. Same shape as the mDNS
/// `DiscoveredPeer` so the frontend renders them in one radar feed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlePeer {
    pub instance_id: String,
    pub display_name: Option<String>,
    pub user_id: Option<String>,
    pub public_key: Option<String>,
    pub rssi: Option<i32>,
    pub last_seen: i64,
}

/// Shared state for the scan task. `peers` is keyed by the
/// peripheral's BD_ADDR string so repeated adverts from the same
/// device overwrite rather than duplicate.
struct ScanState {
    peers: Arc<RwLock<HashMap<String, BlePeer>>>,
}

/// Lazy-initialized singleton so the scan starts on first frontend
/// call and stays running for the lifetime of the app.
static SCANNER: OnceCell<ScanState> = OnceCell::const_new();

async fn ensure_scanner() -> Result<&'static ScanState, String> {
    SCANNER
        .get_or_try_init(|| async {
            let peers = Arc::new(RwLock::new(HashMap::new()));
            let state = ScanState { peers: peers.clone() };
            // Spawn the actual scan loop. Any failure to open the
            // adapter is logged and the task exits; the shared
            // peers map stays empty and the frontend just shows no
            // BLE peers, which is the correct fallback.
            tokio::spawn(async move {
                if let Err(e) = scan_loop(peers).await {
                    tracing::warn!("ble scan loop exited: {e}");
                }
            });
            Ok::<ScanState, String>(state)
        })
        .await
}

async fn scan_loop(peers: Arc<RwLock<HashMap<String, BlePeer>>>) -> Result<(), String> {
    let manager = Manager::new().await.map_err(|e| format!("ble manager: {e}"))?;
    let adapters = manager
        .adapters()
        .await
        .map_err(|e| format!("ble adapters: {e}"))?;
    let central = adapters
        .into_iter()
        .next()
        .ok_or_else(|| "no ble adapter available".to_string())?;

    let travis_uuid = Uuid::from_str(TRAVIS_SERVICE_UUID)
        .map_err(|e| format!("bad service uuid: {e}"))?;
    let filter = ScanFilter { services: vec![travis_uuid] };

    // Subscribe to events BEFORE starting the scan so we don't miss
    // the initial burst of already-known peripherals.
    let mut events = central
        .events()
        .await
        .map_err(|e| format!("ble events: {e}"))?;
    central
        .start_scan(filter)
        .await
        .map_err(|e| format!("ble start_scan: {e}"))?;

    let seen = Arc::new(Mutex::new(HashMap::<String, ()>::new()));

    while let Some(event) = events.next().await {
        match event {
            CentralEvent::DeviceDiscovered(id) | CentralEvent::DeviceUpdated(id) => {
                let peripheral = match central.peripheral(&id).await {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let props = match peripheral.properties().await {
                    Ok(Some(p)) => p,
                    _ => continue,
                };
                // Filter more strictly: services in the advert must
                // include the Travis service UUID. btleplug's scan
                // filter is a hint on some backends (WinRT ignores
                // it), so double-check here.
                let advertises_travis =
                    props.services.contains(&travis_uuid);
                if !advertises_travis {
                    continue;
                }
                let key = id.to_string();
                let peer = BlePeer {
                    instance_id: key.clone(),
                    display_name: props.local_name.clone(),
                    // user_id + public_key come from the identity
                    // characteristic which we don't read yet (that
                    // needs a GATT connect, adds latency). v0.28.52
                    // wires the read once handshake is in.
                    user_id: None,
                    public_key: None,
                    rssi: props.rssi.map(|r| r as i32),
                    last_seen: chrono::Utc::now().timestamp(),
                };
                {
                    let mut map = peers.write().await;
                    map.insert(key.clone(), peer);
                }
                seen.lock().await.insert(key, ());
            }
            CentralEvent::DeviceDisconnected(id) => {
                let key = id.to_string();
                peers.write().await.remove(&key);
                seen.lock().await.remove(&key);
            }
            _ => {}
        }
    }
    Ok(())
}

/// Read the current BLE peer list. Starts the scan on first call.
pub async fn scan_peers() -> Vec<BlePeer> {
    match ensure_scanner().await {
        Ok(state) => state.peers.read().await.values().cloned().collect(),
        Err(e) => {
            tracing::debug!("ble scanner unavailable: {e}");
            Vec::new()
        }
    }
}

/// Placeholder advertise. Cross-platform peripheral role is out of
/// btleplug's scope; v0.28.52 introduces a peripheral crate on top.
pub fn start_advertise(
    _display_name: &str,
    _user_id: Option<&str>,
    _public_key: Option<&str>,
) -> Result<(), String> {
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    pub transfer_id: String,
    pub transport: TransferTransport,
    pub bytes: u64,
    pub total_bytes: Option<u64>,
    pub state: TransferState,
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
    Handshaking,
    Streaming,
    Done,
    Failed,
    Cancelled,
}

/// Placeholder for the BLE file send. v0.28.52 replaces this with
/// the real handshake + chunk pipeline built on the characteristic
/// UUIDs above.
pub fn send_file_ble(
    _peer_instance_id: &str,
    _path: &str,
) -> Result<String, String> {
    Err("BLE file transfer will ship in v0.28.52 alongside the \
         peripheral-role advertise. Scan-only pairs one-way for now."
        .into())
}
