//! What the core keeps of a pairing (design §3.3): `link.json` and `approvals.json` in the app's
//! private storage, each written atomically (a temp file, synced, then renamed). All of it is
//! public: the private key stays in the Keystore.
//!
//! - **`link.json`** ([`LinkFile`]): the gateway's name as the link gave it, the `fp` it pinned,
//!   lmgw-client's [`Credentials`] (the device's certificate, lmgw's CA, the dates, the URL list,
//!   the confirmation code), the Keystore alias and the key's SubjectPublicKeyInfo, the URL that last
//!   connected, the feed's cursor, and the Kai folder the conversation found (the remembered id,
//!   kai §4: a gateway's own ids, so it lives with the pairing).
//! - **`approvals.json`** ([`ApprovalsFile`]): the approval ids the user dismissed, cut at each
//!   `caught_up` to the ids still open, so it never holds more than lmgw's open set (§5.4).
//! - **`calls.json`** ([`CallsFile`]): the voice calls lmgw may still hold for this device (§7.8):
//!   the live call's id from lmgw's answer on, and each call that ended without lmgw's word until
//!   its hangup answered, so a process that dies mid-call still hangs its call up before the next
//!   bind. Ids of an older pairing are answered 404 by any lmgw, as good as hung up.

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use lmgw_client::{pairing::Fingerprint, requests::Credentials};
use serde::{Deserialize, Serialize};

pub const LINK_FILE: &str = "link.json";
pub const APPROVALS_FILE: &str = "approvals.json";
pub const CALLS_FILE: &str = "calls.json";

/// `link.json` (module doc).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkFile {
    /// The device's name as the link gave it (lmgw's row; `hello` names it again).
    pub name: String,
    /// The `fp` the link pinned (`sha256:<64 hex>`): which gateway this is.
    pub fp: String,
    /// The Keystore alias of the device key.
    pub alias: String,
    /// The key's DER SubjectPublicKeyInfo, standard base64: what rustls's signing key reports.
    pub public_key: String,
    /// lmgw-client's enrolment answer as the device keeps it.
    pub credentials: Credentials,
    /// The URL that last connected (design §5.2: tried first).
    #[serde(default)]
    pub last_good: Option<String>,
    /// The feed's cursor: the id of the last stored event the core applied (§5.4). `None` after a
    /// pairing: the first feed starts at "now".
    #[serde(default)]
    pub cursor: Option<String>,
    /// The Kai folder the conversation remembers (kai §4's remembered id).
    #[serde(default)]
    pub folder_id: Option<i64>,
}

impl LinkFile {
    /// Whether the parts hold together as an enrolment left them: `fp` reads and pins the CA
    /// certificate kept beside it, the device's certificate is there, the public key reads.
    pub fn check(&self) -> Result<(), String> {
        let fp = Fingerprint::parse(&self.fp).map_err(|e| format!("holds no fp ({e})"))?;
        if !fp.pins_certificate(&self.credentials.ca_certificate) {
            return Err("its fp does not pin the CA certificate kept with it".to_owned());
        }
        if self.credentials.certificate.is_empty() {
            return Err("it holds no device certificate".to_owned());
        }
        if self.spki().is_none() {
            return Err("its public key is not base64".to_owned());
        }
        if self.alias.trim().is_empty() {
            return Err("it names no Keystore alias".to_owned());
        }
        Ok(())
    }

    /// The device key's DER SubjectPublicKeyInfo.
    pub fn spki(&self) -> Option<Vec<u8>> {
        lmgw_client::base64::decode(&self.public_key).filter(|k| !k.is_empty())
    }
}

/// `approvals.json` (module doc).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ApprovalsFile {
    /// The approval ids whose notification the user dismissed.
    pub dismissed: Vec<String>,
}

/// `calls.json` (module doc).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CallsFile {
    /// The call ids to hang up before the next bind (the live call's while it runs), oldest first.
    pub owed: Vec<String>,
}

/// Reads `path` as JSON; `Ok(None)` when it does not exist. A file that does not read is an error
/// naming it.
pub fn read<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| format!("{} does not read ({e})", path.display()))
}

/// Writes `value` to `path` atomically: a temp file beside it, synced, then renamed over it.
pub fn write<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let mut text = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    text.push(b'\n');
    write_bytes(path, &text)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let tmp = tmp_path(path);
    let result = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    result.map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("cannot write {}: {e}", path.display())
    })
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

/// Removes `path`; a file that is not there is no error.
pub fn remove(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("cannot remove {}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_write_is_atomic_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(APPROVALS_FILE);
        assert_eq!(read::<ApprovalsFile>(&p).unwrap(), None);
        let a = ApprovalsFile {
            dismissed: vec!["mcpr_1".into()],
        };
        write(&p, &a).unwrap();
        assert_eq!(read::<ApprovalsFile>(&p).unwrap(), Some(a));
        assert!(!dir.path().join("approvals.json.tmp").exists());
        std::fs::write(&p, "{ nope").unwrap();
        assert!(
            read::<ApprovalsFile>(&p)
                .unwrap_err()
                .contains("approvals.json")
        );
        remove(&p).unwrap();
        remove(&p).unwrap();
    }
}
