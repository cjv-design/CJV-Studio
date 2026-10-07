//! Original Sony SR2 white balance, before the decoder replaces it with unity
//! for reduced-resolution ARW pixels that already have white balance applied.
//! Format reference: Rawler arw.rs (934af4b), LGPL-2.1; the SR2 stream
//! recurrence below is compatible with that decoder. No image pixels are read.

fn u16_at(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        data.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

// TIFF offsets in the decrypted SR2 directory are relative to the original file.
fn entry(data: &[u8], directory: usize, base: usize, wanted: u16) -> Option<(u16, usize, &[u8])> {
    let count = u16_at(data, directory)? as usize;
    let start = directory.checked_add(2)?;
    data.get(start..start.checked_add(count.checked_mul(12)?)?)?;
    for index in 0..count {
        let pos = start + index * 12;
        if u16_at(data, pos)? != wanted {
            continue;
        }
        let kind = u16_at(data, pos + 2)?;
        let count = u32_at(data, pos + 4)? as usize;
        let unit: usize = match kind {
            1 | 2 | 7 => 1,
            3 | 8 => 2,
            4 | 9 => 4,
            _ => return None,
        };
        let bytes = count.checked_mul(unit)?;
        let offset = if bytes <= 4 {
            pos + 8
        } else {
            (u32_at(data, pos + 8)? as usize).checked_sub(base)?
        };
        return Some((kind, count, data.get(offset..offset.checked_add(bytes)?)?));
    }
    None
}

fn scalar(data: &[u8], directory: usize, wanted: u16) -> Option<u32> {
    let (kind, count, bytes) = entry(data, directory, 0, wanted)?;
    match (kind, count) {
        (4, 1) | (7, 4) | (1, 4) => u32_at(bytes, 0),
        _ => None,
    }
}

fn decode_sr2(input: &[u8], key: u32) -> Vec<u8> {
    let mut state = [0u32; 128];
    let mut seed = key;
    for slot in &mut state[..4] {
        seed = seed.wrapping_mul(48_828_125).wrapping_add(1);
        *slot = seed;
    }
    state[3] = (state[3] << 1) | ((state[0] ^ state[2]) >> 31);
    for i in 4..127 {
        state[i] = ((state[i - 4] ^ state[i - 2]) << 1) | ((state[i - 3] ^ state[i - 1]) >> 31);
    }
    for word in &mut state[..127] {
        *word = word.swap_bytes();
    }
    let mut output = Vec::with_capacity(input.len());
    for (i, bytes) in input.chunks_exact(4).enumerate() {
        let slot = (i + 127) & 127;
        state[slot] = state[(slot + 1) & 127] ^ state[(slot + 65) & 127];
        let value = u32::from_le_bytes(bytes.try_into().unwrap()) ^ state[slot];
        output.extend_from_slice(&value.to_le_bytes());
    }
    output
}

pub fn read_coefficients(file: &[u8]) -> Option<[f32; 4]> {
    if file.get(..4)? != b"II\x2a\0" {
        return None;
    }
    let root = u32_at(file, 4)? as usize;
    let private = scalar(file, root, 50740)? as usize;
    let offset = scalar(file, private, 0x7200)? as usize;
    let length = scalar(file, private, 0x7201)? as usize;
    let key = scalar(file, private, 0x7221)?;
    // Camera metadata is small. Reject corrupt sizes before allocating a buffer.
    if length < 4 || length > 1024 * 1024 {
        return None;
    }
    let decoded = decode_sr2(file.get(offset..offset.checked_add(length)?)?, key);
    let (grbg, (kind, count, bytes)) = match entry(&decoded, 0, offset, 0x7303) {
        Some(value) => (true, value),
        None => (false, entry(&decoded, 0, offset, 0x7313)?),
    };
    if count != 4 {
        return None;
    }
    let mut levels = [0.0f32; 4];
    for (i, level) in levels.iter_mut().enumerate() {
        *level = match kind {
            3 => u16_at(bytes, i * 2)? as f32,
            8 => (u16_at(bytes, i * 2)? as i16) as f32,
            4 => u32_at(bytes, i * 4)? as f32,
            9 => (u32_at(bytes, i * 4)? as i32) as f32,
            _ => return None,
        };
        if *level <= 0.0 {
            return None;
        }
    }
    let [r, g1, g2, b] = if grbg {
        [levels[1], levels[0], levels[3], levels[2]]
    } else {
        levels
    };
    Some([r / g1, (g1 + g2) / (2.0 * g1), b / g1, f32::NAN])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_entry(data: &mut [u8], pos: usize, tag: u16, kind: u16, count: u32, value: u32) {
        data[pos..pos + 2].copy_from_slice(&tag.to_le_bytes());
        data[pos + 2..pos + 4].copy_from_slice(&kind.to_le_bytes());
        data[pos + 4..pos + 8].copy_from_slice(&count.to_le_bytes());
        data[pos + 8..pos + 12].copy_from_slice(&value.to_le_bytes());
    }

    fn fixture(grbg: bool, signed: bool) -> Vec<u8> {
        let mut data = vec![0u8; 192];
        data[..4].copy_from_slice(b"II\x2a\0");
        data[4..8].copy_from_slice(&8u32.to_le_bytes());
        data[8..10].copy_from_slice(&1u16.to_le_bytes());
        put_entry(&mut data, 10, 50740, 7, 4, 32);
        data[32..34].copy_from_slice(&3u16.to_le_bytes());
        put_entry(&mut data, 34, 0x7200, 4, 1, 128);
        put_entry(&mut data, 46, 0x7201, 4, 1, 64);
        put_entry(&mut data, 58, 0x7221, 7, 4, 0x1234abcd);
        let mut clear = vec![0u8; 64];
        clear[..2].copy_from_slice(&1u16.to_le_bytes());
        put_entry(
            &mut clear,
            2,
            if grbg { 0x7303 } else { 0x7313 },
            if signed { 8 } else { 4 },
            4,
            160,
        );
        let levels: [u32; 4] = if grbg {
            [1024, 2048, 1536, 1024]
        } else {
            [2048, 1024, 1024, 1536]
        };
        for (i, value) in levels.iter().enumerate() {
            if signed {
                clear[32 + i * 2..34 + i * 2].copy_from_slice(&(*value as i16).to_le_bytes());
            } else {
                clear[32 + i * 4..36 + i * 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        data[128..].copy_from_slice(&decode_sr2(&clear, 0x1234abcd));
        data
    }

    #[test]
    fn reads_both_sony_channel_orders_and_integer_encodings() {
        for grbg in [false, true] {
            for signed in [false, true] {
                let result = read_coefficients(&fixture(grbg, signed)).unwrap();
                assert_eq!(&result[..3], &[2.0, 1.0, 1.5]);
                assert!(result[3].is_nan());
            }
        }
    }

    #[test]
    fn truncated_or_malformed_metadata_is_rejected_without_panicking() {
        let valid = fixture(false, true);
        for end in 0..valid.len() {
            assert!(read_coefficients(&valid[..end]).is_none());
        }
        for pos in [4, 18, 42, 54] {
            let mut corrupt = valid.clone();
            corrupt[pos..pos + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(read_coefficients(&corrupt).is_none());
        }
        let mut other_format = valid;
        other_format[..2].copy_from_slice(b"MM");
        assert!(read_coefficients(&other_format).is_none());
    }

    #[test]
    fn rejects_zero_and_negative_multipliers() {
        for level in [0i16, -1] {
            let mut data = fixture(false, true);
            let mut clear = decode_sr2(&data[128..], 0x1234abcd);
            clear[32..34].copy_from_slice(&level.to_le_bytes());
            data[128..].copy_from_slice(&decode_sr2(&clear, 0x1234abcd));
            assert!(read_coefficients(&data).is_none());
        }
    }

    #[test]
    fn accepts_sr2_blocks_with_unencrypted_trailing_bytes() {
        // Sony ILCE-7RM5 blocks can end with two bytes beyond the last word.
        let mut data = fixture(false, true);
        data.extend_from_slice(&[0, 0]);
        put_entry(&mut data, 46, 0x7201, 4, 1, 66);
        let result = read_coefficients(&data).unwrap();
        assert_eq!(&result[..3], &[2.0, 1.0, 1.5]);
    }
}
