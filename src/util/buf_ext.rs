use bytes::{Buf, BufMut};
use uuid::Uuid;

pub trait BufExt {
    fn get_uuid(&mut self) -> Uuid;
    fn get_varint(&mut self) -> i32;
    fn get_byte_array(&mut self) -> Vec<u8>;
    fn get_string(&mut self) -> String;
}

impl<T: Buf> BufExt for T {
    fn get_uuid(&mut self) -> Uuid {
        if self.remaining() < 16 {
            return Uuid::nil();
        }
        let most = self.get_u64();
        let least = self.get_u64();
        Uuid::from_u64_pair(most, least)
    }

    fn get_varint(&mut self) -> i32 {
        let mut num_read = 0;
        let mut result = 0u32;
        loop {
            if self.remaining() < 1 {
                return 0;
            }
            let read = self.get_u8();
            if num_read == 4 && read & 0xf0 != 0 {
                return 0;
            }
            let value = (read & 0b0111_1111) as u32;
            result |= value << (7 * num_read);

            num_read += 1;
            if num_read > 5 {
                return 0; // Error case, avoiding panic since it's external client data
            }
            if (read & 0b1000_0000) == 0 {
                break;
            }
        }
        result as i32
    }

    fn get_byte_array(&mut self) -> Vec<u8> {
        let len = self.get_varint() as usize;
        if self.remaining() < len {
            return Vec::new();
        }
        let mut dst = vec![0u8; len];
        self.copy_to_slice(&mut dst);
        dst
    }

    fn get_string(&mut self) -> String {
        let len = self.get_varint() as usize;
        if self.remaining() < len {
            return String::new();
        }
        let mut dst = vec![0u8; len];
        self.copy_to_slice(&mut dst);
        String::from_utf8_lossy(&dst).into_owned()
    }
}

pub trait BufMutExt {
    fn put_uuid(&mut self, uuid: Uuid);
    fn put_varint(&mut self, value: i32);
    fn put_byte_array(&mut self, data: &[u8]);
    fn put_string(&mut self, data: &str);
}

impl<T: BufMut> BufMutExt for T {
    fn put_uuid(&mut self, uuid: Uuid) {
        let (most, least) = uuid.as_u64_pair();
        self.put_u64(most);
        self.put_u64(least);
    }

    fn put_varint(&mut self, value: i32) {
        let mut val = value as u32;
        while val > 0x7F {
            self.put_u8((val as u8) | 0x80);
            val >>= 7;
        }
        self.put_u8(val as u8);
    }

    fn put_byte_array(&mut self, data: &[u8]) {
        self.put_varint(data.len() as i32);
        self.put_slice(data);
    }

    fn put_string(&mut self, data: &str) {
        let bytes = data.as_bytes();
        self.put_varint(bytes.len() as i32);
        self.put_slice(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::BufExt;

    #[test]
    fn oversized_varints_do_not_shift_past_u32() {
        assert_eq!((&[0x80; 6][..]).get_varint(), 0);
        assert_eq!((&[0xff, 0xff, 0xff, 0xff, 0x7f][..]).get_varint(), 0);
        assert_eq!((&[0xff, 0xff, 0xff, 0xff, 0x0f][..]).get_varint(), -1);
    }
}
