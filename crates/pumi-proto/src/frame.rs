use serde::Serialize;
use serde::de::DeserializeOwned;
use std::io::{self, Read, Write};
pub const MAX_FRAME: usize = 64 << 20;
pub fn write<W: Write, T: Serialize>(writer: &mut W, value: &T) -> io::Result<()> {
    let bytes = encode(value)?;
    writer.write_all(&bytes)?;
    writer.flush()
}
pub fn encode<T: Serialize>(value: &T) -> io::Result<Vec<u8>> {
    let body = postcard::to_stdvec(value).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    if body.len() > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut bytes = Vec::with_capacity(body.len() + 4);
    bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&body);
    Ok(bytes)
}
pub fn read<R: Read, T: DeserializeOwned>(reader: &mut R) -> io::Result<Option<T>> {
    let mut len = [0u8; 4];
    match reader.read_exact(&mut len) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err),
    }
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    postcard::from_bytes(&body).map(Some).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frames_round_trip_and_end_cleanly() {
        let mut buf = Vec::new();
        write(&mut buf, &("a".to_string(), 7u64)).unwrap();
        write(&mut buf, &vec![1u8, 2, 3]).unwrap();
        let mut cursor = io::Cursor::new(buf);
        assert_eq!(read::<_, (String, u64)>(&mut cursor).unwrap(), Some(("a".into(), 7)));
        assert_eq!(read::<_, Vec<u8>>(&mut cursor).unwrap(), Some(vec![1, 2, 3]));
        assert_eq!(read::<_, Vec<u8>>(&mut cursor).unwrap(), None);
    }
    #[test]
    fn oversized_frames_are_refused() {
        let mut cursor = io::Cursor::new(((MAX_FRAME + 1) as u32).to_le_bytes().to_vec());
        assert!(read::<_, Vec<u8>>(&mut cursor).is_err());
    }
}
