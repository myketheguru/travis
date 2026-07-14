//! v0.28.53 — crypto primitives for T2T secure file transfer.
//!
//! Design goals:
//!  - **End-to-end**: cloud sees only ciphertext + ephemeral pubkey.
//!    Even a full server compromise can't decrypt past traffic.
//!  - **Static-ephemeral X25519**: each user has a long-lived X25519
//!    keypair. The private half lives in the OS keyring so it never
//!    hits disk in plaintext. The public half is published to the
//!    cloud once so senders can find it. Senders generate a fresh
//!    ephemeral keypair per transfer, do ECDH against the recipient's
//!    static pubkey, and derive a session key via HKDF-SHA256.
//!  - **Single-shot AEAD**: the whole file goes through one
//!    ChaCha20-Poly1305 seal with a fixed nonce (12 bytes derived
//!    from the transfer id via HKDF's "info"). Files are capped at
//!    25 MB by the server; chunked/streaming AEAD can be layered in
//!    a follow-up if that limit ever bites.
//!  - **No custom crypto**: X25519 via `x25519-dalek`, HKDF via
//!    `hkdf`, AEAD via `chacha20poly1305`. Sensitive material
//!    zeroized on drop via `zeroize`.
//!
//! The transfer id + salt tag ("travis-t2t-v1|<transferId>") also
//! authenticates the framing — a tampered R2 object or a swapped
//! ephemeral pubkey both cause decryption to fail loudly.

use anyhow::{anyhow, Result};
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use hkdf::Hkdf;
use rand_core::{OsRng, RngCore};
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Keyring service + username identifiers for the static X25519 secret.
/// One entry per install; a user signing out doesn't wipe this — the
/// key is tied to the machine, not the account. If a fresh key is
/// desired, delete this entry and let init() re-mint.
const KEYRING_SERVICE: &str = "com.myketheguru.travis.crypto";
const KEYRING_USER: &str = "x25519.static";

const HKDF_INFO_PREFIX: &[u8] = b"travis-t2t-v1|";

/// Deterministic nonce constructor. Same transfer id → same nonce,
/// but since the session key is per-transfer via HKDF salt, key+nonce
/// pairs are still unique across transfers.
fn derive_nonce_from_transfer_id(transfer_id: &str) -> [u8; 12] {
    let hk = Hkdf::<Sha256>::new(Some(transfer_id.as_bytes()), b"travis-t2t-nonce");
    let mut nonce = [0u8; 12];
    // OKM is domain-separated from the key derivation because the
    // salt + info here are different from the session-key derivation.
    let _ = hk.expand(b"nonce", &mut nonce);
    nonce
}

/// Wrapper that zeroes the byte buffer on drop.
#[derive(ZeroizeOnDrop)]
struct SessionKey([u8; 32]);

/// Load the on-disk static X25519 keypair; create + persist one if
/// this machine has never generated one. Persisted in the OS keyring
/// as raw hex.
pub fn load_or_create_static_keypair() -> Result<StaticSecret> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?;
    match entry.get_password() {
        Ok(hexed) => {
            let bytes = hex::decode(hexed.trim()).map_err(|e| anyhow!("decode hex: {e}"))?;
            if bytes.len() != 32 {
                anyhow::bail!("keyring x25519 secret has wrong length");
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            let secret = StaticSecret::from(arr);
            arr.zeroize();
            Ok(secret)
        }
        Err(keyring::Error::NoEntry) => {
            let secret = StaticSecret::random_from_rng(OsRng);
            let hexed = hex::encode(secret.to_bytes());
            entry
                .set_password(&hexed)
                .map_err(|e| anyhow!("keyring set: {e}"))?;
            Ok(secret)
        }
        Err(e) => Err(anyhow!("keyring get: {e}")),
    }
}

/// Public key derived from the machine's static X25519 secret, as
/// lowercase hex — the format the cloud endpoint expects.
pub fn static_public_hex() -> Result<String> {
    let secret = load_or_create_static_keypair()?;
    let public = PublicKey::from(&secret);
    Ok(hex::encode(public.as_bytes()))
}

/// Sender side. Given the recipient's static pubkey and a transfer id,
/// mint an ephemeral X25519 secret, do ECDH, HKDF-derive a session
/// key, and ChaCha20-Poly1305-seal the plaintext.
///
/// Returns `(ciphertext, ephemeral_public_hex)` so the caller can
/// attach the ephem pubkey to the upload.
pub fn encrypt_for_recipient(
    recipient_pubkey_hex: &str,
    transfer_id: &str,
    plaintext: &[u8],
) -> Result<(Vec<u8>, String)> {
    let recipient_bytes =
        hex::decode(recipient_pubkey_hex.trim()).map_err(|e| anyhow!("decode recipient: {e}"))?;
    if recipient_bytes.len() != 32 {
        anyhow::bail!("recipient pubkey wrong length");
    }
    let mut recipient_arr = [0u8; 32];
    recipient_arr.copy_from_slice(&recipient_bytes);
    let recipient_pk = PublicKey::from(recipient_arr);

    // Ephemeral keypair — fresh for every transfer. StaticSecret gives
    // us a `to_bytes()` scrub path via zeroize.
    let ephem_secret = StaticSecret::random_from_rng(OsRng);
    let ephem_pub = PublicKey::from(&ephem_secret);
    let shared = ephem_secret.diffie_hellman(&recipient_pk);

    let session_key = derive_session_key(shared.as_bytes(), transfer_id);
    let cipher = ChaCha20Poly1305::new(session_key.0.as_slice().into());
    let nonce_bytes = derive_nonce_from_transfer_id(transfer_id);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .map_err(|e| anyhow!("aead encrypt: {e}"))?;

    Ok((ciphertext, hex::encode(ephem_pub.as_bytes())))
}

/// Recipient side. Given the sender's ephemeral pubkey (hex) and the
/// transfer id, ECDH with the recipient's own static secret, derive
/// the same session key, and open the ciphertext.
pub fn decrypt_from_sender(
    sender_ephem_pubkey_hex: &str,
    transfer_id: &str,
    ciphertext: &[u8],
) -> Result<Vec<u8>> {
    let my_secret = load_or_create_static_keypair()?;
    let ephem_bytes =
        hex::decode(sender_ephem_pubkey_hex.trim()).map_err(|e| anyhow!("decode ephem: {e}"))?;
    if ephem_bytes.len() != 32 {
        anyhow::bail!("ephem pubkey wrong length");
    }
    let mut ephem_arr = [0u8; 32];
    ephem_arr.copy_from_slice(&ephem_bytes);
    let ephem_pk = PublicKey::from(ephem_arr);
    let shared = my_secret.diffie_hellman(&ephem_pk);

    let session_key = derive_session_key(shared.as_bytes(), transfer_id);
    let cipher = ChaCha20Poly1305::new(session_key.0.as_slice().into());
    let nonce_bytes = derive_nonce_from_transfer_id(transfer_id);
    cipher
        .decrypt(Nonce::from_slice(&nonce_bytes), ciphertext)
        .map_err(|e| anyhow!("aead decrypt (tag mismatch — tampered or wrong key): {e}"))
}

fn derive_session_key(dh_output: &[u8; 32], transfer_id: &str) -> SessionKey {
    // Salt = transfer id (unique per transfer), info = version tag +
    // transfer id (rebinds the key to this specific transfer).
    let hk = Hkdf::<Sha256>::new(Some(transfer_id.as_bytes()), dh_output);
    let mut key = [0u8; 32];
    let mut info = Vec::with_capacity(HKDF_INFO_PREFIX.len() + transfer_id.len());
    info.extend_from_slice(HKDF_INFO_PREFIX);
    info.extend_from_slice(transfer_id.as_bytes());
    hk.expand(&info, &mut key)
        .expect("hkdf expand of fixed length");
    SessionKey(key)
}

/// Generate a per-transfer random id — used both as the crypto salt
/// and as the R2 object name. 16 bytes of urandom, hex-encoded.
pub fn new_transfer_id() -> String {
    let mut buf = [0u8; 16];
    OsRng.fill_bytes(&mut buf);
    hex::encode(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        // Simulate two peers by inlining static secrets rather than
        // touching the OS keyring (which unit tests shouldn't touch).
        let alice = StaticSecret::random_from_rng(OsRng);
        let alice_pub_hex = hex::encode(PublicKey::from(&alice).as_bytes());

        let transfer_id = new_transfer_id();
        let plaintext = b"Hello, Travis. Attached: your grocery list.";
        let (ciphertext, ephem_pub_hex) =
            encrypt_for_recipient(&alice_pub_hex, &transfer_id, plaintext).unwrap();
        // We can't decrypt via `decrypt_from_sender` in this test because it
        // reads the keyring — but we can verify the crypto plumbs by
        // reversing ECDH manually with the test alice secret.
        let ephem_bytes = hex::decode(&ephem_pub_hex).unwrap();
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&ephem_bytes);
        let shared = alice.diffie_hellman(&PublicKey::from(arr));
        let session_key = derive_session_key(shared.as_bytes(), &transfer_id);
        let cipher = ChaCha20Poly1305::new(session_key.0.as_slice().into());
        let nonce = derive_nonce_from_transfer_id(&transfer_id);
        let recovered = cipher
            .decrypt(Nonce::from_slice(&nonce), ciphertext.as_slice())
            .unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn tamper_detection() {
        let alice = StaticSecret::random_from_rng(OsRng);
        let alice_pub_hex = hex::encode(PublicKey::from(&alice).as_bytes());
        let transfer_id = new_transfer_id();
        let (mut ciphertext, ephem_pub_hex) =
            encrypt_for_recipient(&alice_pub_hex, &transfer_id, b"tamper me").unwrap();
        // Flip one byte.
        ciphertext[5] ^= 0xff;
        let ephem_bytes = hex::decode(&ephem_pub_hex).unwrap();
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&ephem_bytes);
        let shared = alice.diffie_hellman(&PublicKey::from(arr));
        let session_key = derive_session_key(shared.as_bytes(), &transfer_id);
        let cipher = ChaCha20Poly1305::new(session_key.0.as_slice().into());
        let nonce = derive_nonce_from_transfer_id(&transfer_id);
        assert!(cipher
            .decrypt(Nonce::from_slice(&nonce), ciphertext.as_slice())
            .is_err());
    }
}
