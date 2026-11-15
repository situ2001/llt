//! Bounds-checked primitives for decoding the package's binary members.
//!
//! Every member decoder and the on-demand shard scans funnel through
//! [`Reader`], so each wire encoding is defined exactly once:
//!
//! - 2-byte 255-base integers (`b0 * 255 + b1`) for counts and indices
//!   ([`Reader::u16_custom`]).
//! - 4-byte little-endian base-128 packed YYYYMMDD dates
//!   ([`Reader::date`]).
//! - Big-endian u32 counters/flags (routes.dat, jlb.dat, sp.dat)
//!   ([`Reader::u32be`]) and Java DataInputStream shorts
//!   ([`Reader::u16be`]).
//! - Length-prefixed UTF-8 strings: 1-byte ([`Reader::str8`]) and Java
//!   `readUTF` 2-byte ([`Reader::str16`]).

use crate::ParseError;

/// Sequential reader over a decoded member with bounds-checked primitives.
pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
    name: &'a str,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8], name: &'a str) -> Reader<'a> {
        Reader {
            bytes,
            pos: 0,
            name,
        }
    }

    pub(crate) fn truncated(&self) -> ParseError {
        ParseError::Truncated(self.name.to_string())
    }

    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], ParseError> {
        if self.pos + n > self.bytes.len() {
            return Err(self.truncated());
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    /// Consume `n` bytes without decoding them (padding, unused tables,
    /// skipped records).
    pub(crate) fn skip(&mut self, n: usize) -> Result<(), ParseError> {
        self.take(n).map(|_| ())
    }

    pub(crate) fn byte(&mut self) -> Result<u8, ParseError> {
        Ok(self.take(1)?[0])
    }

    /// Big-endian u32, the counter/flag encoding of routes.dat and jlb.dat.
    pub(crate) fn u32be(&mut self) -> Result<u32, ParseError> {
        let bytes = self.take(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Big-endian u16, the short encoding of sn.dat (Java DataInputStream).
    pub(crate) fn u16be(&mut self) -> Result<u16, ParseError> {
        let bytes = self.take(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    /// The 255-base 2-byte encoding used for counts and indices throughout
    /// the package.
    pub(crate) fn u16_custom(&mut self) -> Result<usize, ParseError> {
        let bytes = self.take(2)?;
        Ok(dec2(bytes[0], bytes[1]))
    }

    /// 4-byte little-endian base-128 packed YYYYMMDD date, as stored in T
    /// records.
    pub(crate) fn date(&mut self) -> Result<u32, ParseError> {
        let bytes = self.take(4)?;
        Ok(read_date(bytes))
    }

    /// Length-prefixed UTF-8 string ([1B len][bytes]).
    pub(crate) fn str8(&mut self) -> Result<String, ParseError> {
        let len = self.byte()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| self.truncated())
    }

    /// Java `readUTF` string ([2B BE len][bytes]).
    pub(crate) fn str16(&mut self) -> Result<String, ParseError> {
        let len = self.u16be()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| self.truncated())
    }

    pub(crate) fn at_end(&self) -> bool {
        self.pos == self.bytes.len()
    }
}

/// The 255-base 2-byte integer encoding used throughout the data files.
fn dec2(hi: u8, lo: u8) -> usize {
    hi as usize * 255 + lo as usize
}

/// 4-byte little-endian base-128 packed YYYYMMDD date.
fn read_date(bytes: &[u8]) -> u32 {
    (bytes[0] as u32)
        + (bytes[1] as u32) * 128
        + (bytes[2] as u32) * 128 * 128
        + (bytes[3] as u32) * 128 * 128 * 128
}
