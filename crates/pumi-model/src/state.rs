use base64::Engine;
use pumi_api::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
/// The version of this shape. Another one is refused, not converted.
pub const STATE_VERSION: u32 = 1;
/// Everything needed to bring a tab back, here or on another server.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TabState {
    pub pumi_state: u32,
    /// The block that saved it.
    #[serde(alias = "type")]
    pub block: String,
    /// The version of that block's `data`.
    #[serde(alias = "type_version")]
    pub block_version: u32,
    /// The tab's own options, `@user` ones included.
    #[serde(default)]
    pub options: BTreeMap<String, Value>,
    /// What the block's `serialize` returned.
    #[serde(default)]
    pub data: Value,
    /// Large parts (scrollback, images), by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub blobs: BTreeMap<String, Blob>,
    /// What other plugins added in `TabSavePre`, by plugin.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
    pub strategy: Strategy,
    /// What must be true where it is restored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<Need>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
}
/// How a state comes back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Strategy {
    /// As it was.
    Exact,
    /// By running something again: a terminal's command, in its directory, under its scrollback.
    Relaunch,
    /// It points at something that must still exist, such as a file.
    Reference,
}
/// A blob: its content's hash, and the content itself when the state travels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Blob {
    /// How to read it: `vt` (terminal output), `png`, `bytes`.
    pub encoding: String,
    pub size: u64,
    /// `blob:<sha256>`.
    #[serde(rename = "ref")]
    pub reference: String,
    /// The bytes, in base64, when the state is to leave this server (`opts.blobs = "inline"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Need {
    Path { path: String },
    Program { name: String },
    Url { url: String },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    pub server: String,
    pub tab: u64,
    /// RFC 3339, UTC.
    pub at: String,
}
impl TabState {
    /// Refuses a state this build cannot restore: another version, block or block version.
    pub fn check(&self, block: &str, version: u32) -> Result<()> {
        if self.pumi_state != STATE_VERSION {
            return Err(Error::invalid_params(format!(
                "a saved state of version {} (this server reads {STATE_VERSION})",
                self.pumi_state
            )));
        }
        if self.block != block {
            return Err(Error::invalid_params(format!(
                "a {} block's state cannot restore a {block} block",
                self.block
            )));
        }
        if self.block_version != version {
            return Err(Error::invalid_params(format!(
                "a {block} state of version {} (this {block} reads {version})",
                self.block_version
            )));
        }
        Ok(())
    }
}
/// Blobs by hash: in the data directory, or in memory for a server that persists nothing.
pub struct Blobs {
    dir: Option<PathBuf>,
    memory: HashMap<String, Vec<u8>>,
}
impl Blobs {
    pub fn new(dir: Option<PathBuf>) -> Blobs {
        if let Some(dir) = &dir {
            let _ = std::fs::create_dir_all(dir);
        }
        Blobs { dir, memory: HashMap::new() }
    }
    /// Keeps `bytes`, returning their blob.
    pub fn put(&mut self, encoding: &str, bytes: &[u8]) -> Blob {
        let hash = hex(&sha2::Sha256::digest(bytes));
        let reference = format!("blob:{hash}");
        match &self.dir {
            Some(dir) => {
                let path = dir.join(&hash);
                if !path.exists() {
                    let tmp = dir.join(format!("{hash}.tmp"));
                    if std::fs::write(&tmp, bytes).is_ok() {
                        let _ = std::fs::rename(&tmp, &path);
                    }
                }
            }
            None => {
                self.memory.entry(reference.clone()).or_insert_with(|| bytes.to_vec());
            }
        }
        Blob { encoding: encoding.into(), size: bytes.len() as u64, reference, data: None }
    }
    /// A blob's bytes: inline in it, or kept here.
    pub fn get(&self, blob: &Blob) -> Result<Vec<u8>> {
        if let Some(data) = &blob.data {
            return base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|err| Error::invalid_params(format!("blob {}: {err}", blob.reference)));
        }
        let missing = || Error::not_found(format!("no blob {}", blob.reference));
        match &self.dir {
            Some(dir) => {
                let hash = blob.reference.strip_prefix("blob:").ok_or_else(missing)?;
                if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Err(missing());
                }
                std::fs::read(dir.join(hash)).map_err(|_| missing())
            }
            None => self.memory.get(&blob.reference).cloned().ok_or_else(missing),
        }
    }
    /// The blob with its bytes in it, for a state that leaves this server.
    pub fn inline(&self, blob: &Blob) -> Result<Blob> {
        let bytes = self.get(blob)?;
        Ok(Blob { data: Some(base64::engine::general_purpose::STANDARD.encode(bytes)), ..blob.clone() })
    }
    /// Forgets every blob not in `keep` (references).
    pub fn retain(&mut self, keep: &std::collections::HashSet<String>) {
        match &self.dir {
            Some(dir) => {
                let Ok(entries) = std::fs::read_dir(dir) else { return };
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if !keep.contains(&format!("blob:{name}")) {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
            None => self.memory.retain(|reference, _| keep.contains(reference)),
        }
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
/// Now, as seconds since the epoch.
pub fn epoch() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}
/// Now, as RFC 3339 in UTC.
pub fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    rfc3339(secs as i64)
}
/// Seconds since the epoch of an RFC 3339 time (`2026-10-16T07:18:39Z`, a fraction and an offset
/// allowed), or `None` when `text` is not one.
pub fn parse_rfc3339(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    let num = |at: usize, len: usize| -> Option<i64> {
        let s = text.get(at..at + len)?;
        s.bytes().all(|c| c.is_ascii_digit()).then(|| s.parse().ok())?
    };
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let (year, month, day) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let (hour, minute, second) = (num(11, 2)?, num(14, 2)?, num(17, 2)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let mut rest = &text[19..];
    if let Some(fraction) = rest.strip_prefix('.') {
        let digits = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        rest = &fraction[digits..];
    }
    let offset = match rest {
        "Z" | "z" => 0,
        _ if rest.len() == 6 && matches!(&rest[..1], "+" | "-") && &rest[3..4] == ":" => {
            let minutes = rest[1..3].parse::<i64>().ok()? * 60 + rest[4..6].parse::<i64>().ok()?;
            if &rest[..1] == "+" { minutes * 60 } else { -minutes * 60 }
        }
        _ => return None,
    };
    let (y, m) = if month <= 2 { (year - 1, month + 9) } else { (year, month - 3) };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second - offset)
}
pub fn rfc3339(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blobs_are_named_by_their_content_and_travel_inline() {
        let dir = tempfile::tempdir().unwrap();
        let mut blobs = Blobs::new(Some(dir.path().to_path_buf()));
        let blob = blobs.put("vt", b"hello\r\n");
        assert!(blob.reference.starts_with("blob:") && blob.reference.len() == 5 + 64);
        assert_eq!(blobs.put("vt", b"hello\r\n"), blob, "the same content is the same blob");
        assert_eq!(blobs.get(&blob).unwrap(), b"hello\r\n");
        let inline = blobs.inline(&blob).unwrap();
        let elsewhere = Blobs::new(None);
        assert_eq!(elsewhere.get(&inline).unwrap(), b"hello\r\n");
        assert!(elsewhere.get(&blob).is_err(), "another server has no copy until it is sent");
        blobs.retain(&Default::default());
        assert!(blobs.get(&blob).is_err());
    }
    #[test]
    fn times_are_rfc3339() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(1_792_135_119), "2026-10-16T07:18:39Z");
        assert_eq!(parse_rfc3339("2026-10-16T07:18:39Z"), Some(1_792_135_119));
        assert_eq!(parse_rfc3339("2026-10-16T09:18:39.5+02:00"), Some(1_792_135_119));
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        for bad in ["2026-10-16", "2026-13-16T07:18:39Z", "2026-10-16T07:18:39", "tomorrow"] {
            assert_eq!(parse_rfc3339(bad), None, "{bad}");
        }
    }
}
