use uuid::Uuid;

/// Bounds-checked reader for untrusted Simple Voice Chat custom payloads.
pub struct PayloadReader<'a> {
    remaining: &'a [u8],
}

impl<'a> PayloadReader<'a> {
    #[must_use]
    pub const fn new(data: &'a [u8]) -> Self {
        Self { remaining: data }
    }

    pub fn read_u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    pub fn read_bool(&mut self) -> Option<bool> {
        Some(self.read_u8()? != 0)
    }

    pub fn read_i16(&mut self) -> Option<i16> {
        Some(i16::from_be_bytes(self.take(2)?.try_into().ok()?))
    }

    pub fn read_i32(&mut self) -> Option<i32> {
        Some(i32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }

    pub fn read_i64(&mut self) -> Option<i64> {
        Some(i64::from_be_bytes(self.take(8)?.try_into().ok()?))
    }

    pub fn read_uuid(&mut self) -> Option<Uuid> {
        Some(Uuid::from_bytes(self.take(16)?.try_into().ok()?))
    }

    pub fn read_varint(&mut self) -> Option<i32> {
        let mut result = 0_u32;
        for byte_index in 0..5 {
            let byte = self.read_u8()?;
            if byte_index == 4 && byte & 0xf0 != 0 {
                return None;
            }
            result |= u32::from(byte & 0x7f) << (byte_index * 7);
            if byte & 0x80 == 0 {
                return Some(result as i32);
            }
        }
        None
    }

    pub fn read_string(&mut self, max_utf16_units: usize) -> Option<String> {
        let byte_length = usize::try_from(self.read_varint()?).ok()?;
        // A UTF-8 scalar needs at most four bytes. This early bound prevents a
        // malicious length from driving an oversized allocation or slice.
        if byte_length > max_utf16_units.saturating_mul(4) {
            return None;
        }
        let value = std::str::from_utf8(self.take(byte_length)?).ok()?;
        if value.encode_utf16().count() > max_utf16_units {
            return None;
        }
        Some(value.to_owned())
    }

    pub fn read_byte_array(&mut self, max_length: usize) -> Option<Vec<u8>> {
        Some(self.read_byte_slice(max_length)?.to_vec())
    }

    /// Borrow an untrusted length-prefixed payload without allocating a copy.
    pub fn read_byte_slice(&mut self, max_length: usize) -> Option<&'a [u8]> {
        let byte_length = usize::try_from(self.read_varint()?).ok()?;
        if byte_length > max_length {
            return None;
        }
        self.take(byte_length)
    }

    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.remaining.is_empty()
    }

    fn take(&mut self, length: usize) -> Option<&'a [u8]> {
        if self.remaining.len() < length {
            return None;
        }
        let (value, remaining) = self.remaining.split_at(length);
        self.remaining = remaining;
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::PayloadReader;

    #[test]
    fn byte_slice_borrows_input_and_enforces_bounds() {
        let input = [3, 10, 20, 30, 99];
        let mut reader = PayloadReader::new(&input);
        let payload = reader.read_byte_slice(3).unwrap();
        assert_eq!(payload, &[10, 20, 30]);
        assert_eq!(payload.as_ptr(), input[1..].as_ptr());
        assert_eq!(reader.read_u8(), Some(99));
        assert!(reader.is_finished());
        assert!(PayloadReader::new(&input).read_byte_slice(2).is_none());
        assert!(PayloadReader::new(&[3, 10]).read_byte_slice(3).is_none());
        assert!(
            PayloadReader::new(&[0xff, 0xff, 0xff, 0xff, 0x0f])
                .read_byte_slice(3)
                .is_none()
        );
    }

    #[test]
    fn rejects_truncated_and_oversized_values_without_panicking() {
        assert!(PayloadReader::new(&[0x80]).read_varint().is_none());
        assert!(
            PayloadReader::new(&[0xff, 0xff, 0xff, 0xff, 0x7f])
                .read_varint()
                .is_none()
        );
        assert!(PayloadReader::new(&[2, b'a']).read_string(24).is_none());
        assert!(
            PayloadReader::new(&[2, b'a', b'b'])
                .read_string(1)
                .is_none()
        );
        assert!(PayloadReader::new(&[2, 1]).read_byte_array(2).is_none());
        assert!(
            PayloadReader::new(&[3, 1, 2, 3])
                .read_byte_array(2)
                .is_none()
        );
    }
}
