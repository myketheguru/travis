//! Travis-to-Travis peer discovery via mDNS/Bonjour (task 314).
//!
//! Every Travis instance advertises itself on the LAN under the
//! service type `_travis._tcp.local.`. Peers browse the same service
//! type to see who's nearby. Discovered peers show up in the
//! attention strip as "Michael's Travis · nearby — pair?" chips;
//! clicking one drops into the existing T2T invite flow.
//!
//! Scope:
//! - Advertise (broadcast) — includes display name + user_id + email
//!   in TXT records so peers see who's on the other side
//! - Browse — collect discovered peers into an in-memory Map
//! - Tauri commands to query the current peer list + start/stop
//!
//! Not shipped:
//! - Encrypted proof of identity (someone on the LAN could spoof)
//!   — MITM protection lands at pair-time via cloud verification
//! - Persistence — peers rediscover on every launch
//! - Cross-subnet discovery (mDNS is LAN-only by nature)

pub mod cmd;

use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task;

const SERVICE_TYPE: &str = "_travis._tcp.local.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPeer {
    pub instance_name: String,
    pub display_name: Option<String>,
    pub user_email: Option<String>,
    pub user_id: Option<String>,
    pub host: String,
    pub port: u16,
    /// UNIX seconds when we last saw this peer.
    pub last_seen: i64,
}

pub struct DiscoveryState {
    daemon: ServiceDaemon,
    peers: Arc<RwLock<HashMap<String, DiscoveredPeer>>>,
    _advertise: Option<ServiceInfo>,
}

impl DiscoveryState {
    /// Start the daemon, register our advertisement, and begin
    /// browsing for peers. Returns immediately; browsing runs in a
    /// background tokio task and updates the shared peer map.
    pub fn start(
        display_name: &str,
        user_email: Option<&str>,
        user_id: Option<&str>,
    ) -> Result<Self> {
        let daemon = ServiceDaemon::new()?;

        // Advertise our presence. The port is symbolic (mDNS requires
        // one); Travis doesn't listen on it since pairing goes through
        // cloud, not direct LAN.
        let instance = format!(
            "travis-{}",
            user_id
                .map(|s| s.chars().take(8).collect::<String>())
                .unwrap_or_else(|| "anon".into())
        );
        let hostname = format!("{}.local.", &instance);
        // TXT records — mdns-sd expects (K, V) tuples. Keys are stable
        // strings; values carry the identity data peers use to display
        // + resolve.
        let mut props: Vec<(&'static str, String)> = Vec::new();
        props.push(("name", display_name.to_string()));
        if let Some(email) = user_email {
            props.push(("email", email.to_string()));
        }
        if let Some(uid) = user_id {
            props.push(("uid", uid.to_string()));
        }

        let service = ServiceInfo::new(
            SERVICE_TYPE,
            &instance,
            &hostname,
            "",
            0,
            &props[..],
        )?
        .enable_addr_auto();
        daemon.register(service.clone())?;

        let peers = Arc::new(RwLock::new(HashMap::new()));

        // Browse — background loop that receives ServiceEvent items and
        // updates the peer map.
        let receiver = daemon.browse(SERVICE_TYPE)?;
        let peers_clone = peers.clone();
        let our_instance = instance.clone();
        task::spawn(async move {
            for event in receiver.iter() {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        let full = info.get_fullname().to_string();
                        // Filter self.
                        if full.contains(&our_instance) {
                            continue;
                        }
                        let props: HashMap<String, String> = info
                            .get_properties()
                            .iter()
                            .map(|p| (p.key().to_string(), p.val_str().to_string()))
                            .collect();
                        let peer = DiscoveredPeer {
                            instance_name: full.clone(),
                            display_name: props.get("name").cloned(),
                            user_email: props.get("email").cloned(),
                            user_id: props.get("uid").cloned(),
                            host: info.get_hostname().to_string(),
                            port: info.get_port(),
                            last_seen: chrono::Utc::now().timestamp(),
                        };
                        peers_clone.write().await.insert(full, peer);
                    }
                    ServiceEvent::ServiceRemoved(_ty, full) => {
                        peers_clone.write().await.remove(&full);
                    }
                    _ => {}
                }
            }
        });

        Ok(Self {
            daemon,
            peers,
            _advertise: Some(service),
        })
    }

    pub async fn peers(&self) -> Vec<DiscoveredPeer> {
        self.peers.read().await.values().cloned().collect()
    }

    pub fn shutdown(&self) {
        // Best-effort clean shutdown; ignored errors since mDNS quiescing
        // is not user-visible.
        let _ = self.daemon.shutdown();
    }
}
