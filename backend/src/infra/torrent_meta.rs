use crate::error::{AppError, AppResult};

/// Info-hashes qBittorrent will accept in `torrents/info?hashes=`.
///
/// v1 is SHA-1 of the raw `info` dict. Hybrid torrents also have a v2 hash;
/// qBittorrent's id is the v1 hash when `pieces` is present, otherwise the
/// first 20 bytes of SHA-256 (libtorrent's truncated v2 id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoHashes {
    pub v1: Option<String>,
    pub v2: Option<String>,
}

impl InfoHashes {
    pub fn preferred(&self) -> &str {
        self.v1.as_deref().or(self.v2.as_deref()).unwrap_or("")
    }

    pub fn candidates(&self) -> impl Iterator<Item = &str> {
        self.v1.iter().chain(self.v2.iter()).map(String::as_str)
    }
}

pub fn info_hashes(torrent: &[u8]) -> AppResult<InfoHashes> {
    let info = info_dict(torrent).map_err(|e| AppError::Internal(format!("torrent: {e}")))?;
    let has_pieces = dict_has_key(info, "pieces");
    let meta_version = dict_int(info, "meta version");

    let mut hashes = InfoHashes { v1: None, v2: None };
    if has_pieces || meta_version != Some(2) {
        hashes.v1 = Some(sha1_hex(info));
    }
    if meta_version == Some(2) {
        hashes.v2 = Some(sha256_160_hex(info));
    }
    if hashes.v1.is_none() && hashes.v2.is_none() {
        hashes.v1 = Some(sha1_hex(info));
    }
    Ok(hashes)
}

fn info_dict(data: &[u8]) -> Result<&[u8], String> {
    if data.first() != Some(&b'd') {
        return Err("not a bencode dict".into());
    }
    let mut i = 1;
    while data.get(i) != Some(&b'e') {
        let key_start = i;
        skip(data, &mut i)?;
        let key = decode_str(&data[key_start..i])?;
        let val_start = i;
        skip(data, &mut i)?;
        if key == "info" {
            return Ok(&data[val_start..i]);
        }
    }
    Err("missing info dict".into())
}

fn dict_has_key(dict: &[u8], want: &str) -> bool {
    dict_visit(dict, |key, _| key == want).unwrap_or(false)
}

fn dict_int(dict: &[u8], want: &str) -> Option<i64> {
    let mut found = None;
    let _ = dict_visit(dict, |key, val| {
        if key == want {
            found = parse_int(val);
            true
        } else {
            false
        }
    });
    found
}

/// Returns true from `visit` to stop. `Ok(true)` means visit stopped early.
fn dict_visit(dict: &[u8], mut visit: impl FnMut(&str, &[u8]) -> bool) -> Result<bool, String> {
    if dict.first() != Some(&b'd') {
        return Err("expected dict".into());
    }
    let mut i = 1;
    while dict.get(i) != Some(&b'e') {
        let key_start = i;
        skip(dict, &mut i)?;
        let key = decode_str(&dict[key_start..i])?;
        let val_start = i;
        skip(dict, &mut i)?;
        if visit(&key, &dict[val_start..i]) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn parse_int(token: &[u8]) -> Option<i64> {
    if token.first() != Some(&b'i') || token.last() != Some(&b'e') || token.len() < 3 {
        return None;
    }
    std::str::from_utf8(&token[1..token.len() - 1])
        .ok()?
        .parse()
        .ok()
}

fn skip(data: &[u8], i: &mut usize) -> Result<(), String> {
    let b = *data.get(*i).ok_or("truncated bencode")?;
    match b {
        b'i' => {
            *i += 1;
            let rest = data.get(*i..).ok_or("truncated int")?;
            let end = rest.iter().position(|c| *c == b'e').ok_or("bad int")?;
            *i += end + 1;
        }
        b'l' | b'd' => {
            *i += 1;
            while data.get(*i) != Some(&b'e') {
                skip(data, i)?;
            }
            *i += 1;
        }
        b'0'..=b'9' => {
            let start = *i;
            while data.get(*i).is_some_and(|c| c.is_ascii_digit()) {
                *i += 1;
            }
            if data.get(*i) != Some(&b':') {
                return Err("bad string".into());
            }
            let len: usize = std::str::from_utf8(&data[start..*i])
                .map_err(|_| "bad string length")?
                .parse()
                .map_err(|_| "bad string length")?;
            *i += 1;
            if data.len() < *i + len {
                return Err("truncated string".into());
            }
            *i += len;
        }
        _ => return Err(format!("bad bencode byte {b}")),
    }
    Ok(())
}

fn decode_str(token: &[u8]) -> Result<String, String> {
    let colon = token.iter().position(|c| *c == b':').ok_or("bad string")?;
    let len: usize = std::str::from_utf8(&token[..colon])
        .map_err(|_| "bad string length")?
        .parse()
        .map_err(|_| "bad string length")?;
    let bytes = token
        .get(colon + 1..colon + 1 + len)
        .ok_or("truncated string")?;
    String::from_utf8(bytes.to_vec()).map_err(|_| "info key is not utf-8".into())
}

fn sha1_hex(data: &[u8]) -> String {
    use sha1::Digest;
    hex(&sha1::Sha1::digest(data))
}

fn sha256_160_hex(data: &[u8]) -> String {
    use sha2::Digest;
    hex(&sha2::Sha256::digest(data)[..20])
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v1_torrent() -> Vec<u8> {
        let info =
            b"d6:lengthi1e4:name4:test12:piece lengthi16384e6:pieces20:aaaaaaaaaaaaaaaaaaaae";
        let mut torrent = b"d8:announce9:http://x/4:info".to_vec();
        torrent.extend_from_slice(info);
        torrent.push(b'e');
        torrent
    }

    #[test]
    fn v1_hash_is_sha1_of_info_dict_only() {
        let hashes = info_hashes(&v1_torrent()).unwrap();
        assert_eq!(
            hashes.v1.as_deref(),
            Some("5593f95e1450b5791b9904e52c7ce511ea92e105")
        );
        assert!(hashes.v2.is_none());
        assert_eq!(hashes.preferred(), hashes.v1.as_deref().unwrap());
    }

    #[test]
    fn v2_only_uses_truncated_sha256() {
        // keys sorted: "meta version" < "name"
        let info = b"d12:meta versioni2e4:name4:teste";
        let mut torrent = b"d4:info".to_vec();
        torrent.extend_from_slice(info);
        torrent.push(b'e');
        let hashes = info_hashes(&torrent).unwrap();
        assert!(hashes.v1.is_none());
        assert_eq!(
            hashes.v2.as_deref(),
            Some("cc7144525a2daec094c604daab9be98b4bfb8a60")
        );
        assert_eq!(hashes.preferred(), hashes.v2.as_deref().unwrap());
    }

    #[test]
    fn hybrid_prefers_v1() {
        let info = b"d12:meta versioni2e4:name4:test6:pieces20:bbbbbbbbbbbbbbbbbbbbe";
        let mut torrent = b"d4:info".to_vec();
        torrent.extend_from_slice(info);
        torrent.push(b'e');
        let hashes = info_hashes(&torrent).unwrap();
        assert!(hashes.v1.is_some());
        assert!(hashes.v2.is_some());
        assert_ne!(hashes.v1, hashes.v2);
        assert_eq!(hashes.preferred(), hashes.v1.as_deref().unwrap());
    }
}
