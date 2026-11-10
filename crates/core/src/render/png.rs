//! The PNG container (RFC 2083) for 8-bit RGB pixels: the signature, then the
//! `IHDR`, `IDAT` and `IEND` chunks. The pixels are zlib-compressed by
//! `miniz_oxide` at a fixed level, so the same pixels give the same bytes.

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

/// The zlib compression level: miniz's default, a balance of size and speed.
const LEVEL: u8 = 6;

/// Encodes `rgb`, rows of `width` pixels of three bytes each, top row first.
pub(super) fn encode(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    let row = width as usize * 3;
    // Each row starts with its filter type; 0 is none.
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for line in rgb.chunks(row.max(1)) {
        raw.push(0);
        raw.extend_from_slice(line);
    }
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    // Bit depth 8, colour type 2 (RGB), deflate, adaptive filtering, no interlace.
    header.extend_from_slice(&[8, 2, 0, 0, 0]);

    let data = miniz_oxide::deflate::compress_to_vec_zlib(&raw, LEVEL);
    let mut out = SIGNATURE.to_vec();
    chunk(&mut out, *b"IHDR", &header);
    chunk(&mut out, *b"IDAT", &data);
    chunk(&mut out, *b"IEND", &[]);
    out
}

/// Appends one chunk: its length, type, data and CRC.
fn chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    #[expect(clippy::cast_possible_truncation, reason = "a tile's data is far below 4 GiB")]
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// CRC-32 (ISO 3309, as PNG uses it), a byte at a time from a table.
fn crc32(bytes: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut table = [0u32; 256];
        let mut n = 0;
        while n < 256 {
            #[expect(clippy::cast_possible_truncation, reason = "n < 256")]
            let mut c = n as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 == 1 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
                k += 1;
            }
            table[n] = c;
            n += 1;
        }
        table
    };
    !bytes.iter().fold(!0u32, |c, &b| TABLE[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
    }

    #[test]
    fn a_one_pixel_image_has_every_chunk_and_inflates_to_its_rows() {
        let png = encode(1, 1, &[1, 2, 3]);
        assert_eq!(png[..8], SIGNATURE);
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(
            png[png.len() - 12..],
            [0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82]
        );
        let idat = 8 + 25;
        let len = u32::from_be_bytes(png[idat..idat + 4].try_into().unwrap()) as usize;
        assert_eq!(&png[idat + 4..idat + 8], b"IDAT");
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&png[idat + 8..idat + 8 + len]);
        assert_eq!(raw.unwrap(), [0, 1, 2, 3]);
    }
}
