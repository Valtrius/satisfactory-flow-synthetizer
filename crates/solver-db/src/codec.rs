use solver_api::Rational;
use thiserror::Error;

const MAX_FIELD_BYTES: usize = 64 * 1024 * 1024;

/// Strict failure while decoding a persistent canonical record.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum DecodeError {
    #[error("persistent record ended before a complete field")]
    Truncated,
    #[error("persistent record contains a field larger than the codec limit")]
    FieldTooLarge,
    #[error("persistent record contains invalid UTF-8")]
    InvalidUtf8,
    #[error("persistent record contains a noncanonical rational: {0}")]
    NonCanonicalRational(String),
    #[error("persistent record contains invalid boolean tag {0}")]
    InvalidBoolean(u8),
    #[error("persistent record has trailing bytes")]
    TrailingBytes,
    #[error("persistent record integer cannot fit this platform")]
    IntegerOverflow,
}

/// Stable big-endian writer for manually versioned persistent records.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Encoder {
    bytes: Vec<u8>,
}

impl Encoder {
    /// Creates an empty record writer.
    #[must_use]
    pub const fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    pub fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    pub fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    /// Writes a u32-length-prefixed byte string.
    ///
    /// # Panics
    ///
    /// Panics only when an in-memory field exceeds the canonical u32 format.
    pub fn bytes(&mut self, value: &[u8]) {
        self.u32(u32::try_from(value.len()).expect("canonical field length must fit u32"));
        self.bytes.extend_from_slice(value);
    }

    pub fn string(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    pub fn rational(&mut self, value: &Rational) {
        self.string(&value.to_string());
    }

    /// Finishes the canonical payload.
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

/// Bounds-checked reader paired with [`Encoder`].
#[derive(Clone, Debug)]
pub struct Decoder<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Decoder<'a> {
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    /// Reads one byte.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::Truncated`] at end of input.
    pub fn u8(&mut self) -> Result<u8, DecodeError> {
        let value = *self.bytes.get(self.cursor).ok_or(DecodeError::Truncated)?;
        self.cursor += 1;
        Ok(value)
    }

    /// Reads a canonical zero/one boolean.
    ///
    /// # Errors
    ///
    /// Returns an error for truncation or any tag other than zero and one.
    pub fn bool(&mut self) -> Result<bool, DecodeError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(DecodeError::InvalidBoolean(value)),
        }
    }

    /// Reads one big-endian `u16`.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::Truncated`] when fewer than two bytes remain.
    pub fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_be_bytes(self.take_array()?))
    }

    /// Reads one big-endian `u32`.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::Truncated`] when fewer than four bytes remain.
    pub fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_be_bytes(self.take_array()?))
    }

    /// Reads one big-endian `u64`.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::Truncated`] when fewer than eight bytes remain.
    pub fn u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_be_bytes(self.take_array()?))
    }

    /// Reads one bounded `u32`-length-prefixed byte field.
    ///
    /// # Errors
    ///
    /// Returns an error for truncation, overflow, or a field above the codec limit.
    pub fn bytes(&mut self) -> Result<&'a [u8], DecodeError> {
        let length = usize::try_from(self.u32()?).map_err(|_| DecodeError::IntegerOverflow)?;
        if length > MAX_FIELD_BYTES {
            return Err(DecodeError::FieldTooLarge);
        }
        self.take_slice(length)
    }

    /// Reads one bounded UTF-8 field.
    ///
    /// # Errors
    ///
    /// Returns an error from [`Self::bytes`] or for invalid UTF-8.
    pub fn string(&mut self) -> Result<&'a str, DecodeError> {
        std::str::from_utf8(self.bytes()?).map_err(|_| DecodeError::InvalidUtf8)
    }

    /// Reads one canonical exact rational string.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid or noncanonical rational representation.
    pub fn rational(&mut self) -> Result<Rational, DecodeError> {
        let encoded = self.string()?;
        let value = encoded
            .parse::<Rational>()
            .map_err(|_| DecodeError::NonCanonicalRational(encoded.to_owned()))?;
        if value.to_string() != encoded {
            return Err(DecodeError::NonCanonicalRational(encoded.to_owned()));
        }
        Ok(value)
    }

    /// Requires the complete payload to have been consumed.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::TrailingBytes`] when unread bytes remain.
    pub fn finish(self) -> Result<(), DecodeError> {
        if self.cursor == self.bytes.len() {
            Ok(())
        } else {
            Err(DecodeError::TrailingBytes)
        }
    }

    fn take_array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        self.take_slice(N)?
            .try_into()
            .map_err(|_| DecodeError::Truncated)
    }

    fn take_slice(&mut self, length: usize) -> Result<&'a [u8], DecodeError> {
        let end = self
            .cursor
            .checked_add(length)
            .ok_or(DecodeError::IntegerOverflow)?;
        let value = self
            .bytes
            .get(self.cursor..end)
            .ok_or(DecodeError::Truncated)?;
        self.cursor = end;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_round_trips_huge_exact_values_and_big_endian_integers() {
        let huge = "123456789012345678901234567890123456789/9876543210987654321"
            .parse::<Rational>()
            .unwrap();
        let mut encoder = Encoder::new();
        encoder.u16(0x1234);
        encoder.u32(0x1234_5678);
        encoder.u64(0x0123_4567_89ab_cdef);
        encoder.bool(true);
        encoder.rational(&huge);
        let payload = encoder.finish();
        assert_eq!(&payload[..6], &[0x12, 0x34, 0x12, 0x34, 0x56, 0x78]);

        let mut decoder = Decoder::new(&payload);
        assert_eq!(decoder.u16().unwrap(), 0x1234);
        assert_eq!(decoder.u32().unwrap(), 0x1234_5678);
        assert_eq!(decoder.u64().unwrap(), 0x0123_4567_89ab_cdef);
        assert!(decoder.bool().unwrap());
        assert_eq!(decoder.rational().unwrap(), huge);
        decoder.finish().unwrap();
    }

    #[test]
    fn decoder_rejects_noncanonical_rationals_trailing_bytes_and_bad_booleans() {
        let mut noncanonical = Encoder::new();
        noncanonical.string("2/2");
        assert!(matches!(
            Decoder::new(&noncanonical.finish()).rational(),
            Err(DecodeError::NonCanonicalRational(_))
        ));

        let mut boolean = Decoder::new(&[2]);
        assert_eq!(boolean.bool(), Err(DecodeError::InvalidBoolean(2)));

        let mut value = Encoder::new();
        value.u8(1);
        value.u8(2);
        let payload = value.finish();
        let mut decoder = Decoder::new(&payload);
        assert_eq!(decoder.u8().unwrap(), 1);
        assert_eq!(decoder.finish(), Err(DecodeError::TrailingBytes));
    }
}
