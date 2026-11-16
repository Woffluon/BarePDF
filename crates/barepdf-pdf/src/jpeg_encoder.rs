//! Zero-dependency baseline JPEG (SOF0, JFIF) encoder in pure Rust.
//!
//! Supports 3-component YCbCr 4:2:0 encoding from tightly packed RGB24 buffers
//! and 1-component grayscale encoding from tightly packed Y8 buffers.

use std::fmt;
use std::sync::LazyLock;

/// Error returned when baseline JPEG encoding fails validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JpegEncodeError {
    ZeroDimensions,
    DimensionsExceedJpegLimit { width: u32, height: u32 },
    InvalidBufferLength { expected: usize, actual: usize },
}

impl fmt::Display for JpegEncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDimensions => f.write_str("JPEG dimensions must be non-zero"),
            Self::DimensionsExceedJpegLimit { width, height } => write!(
                f,
                "image dimensions {width}x{height} exceed baseline JPEG 65535 limit"
            ),
            Self::InvalidBufferLength { expected, actual } => write!(
                f,
                "pixel buffer length {actual} does not match expected {expected} bytes"
            ),
        }
    }
}

impl std::error::Error for JpegEncodeError {}

#[rustfmt::skip]
const ZIGZAG: [usize; 64] = [
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

#[rustfmt::skip]
const STD_LUMA_QT: [u8; 64] = [
    16, 11, 10, 16,  24,  40,  51,  61,
    12, 12, 14, 19,  26,  58,  60,  55,
    14, 13, 16, 24,  40,  57,  69,  56,
    14, 17, 22, 29,  51,  87,  80,  62,
    18, 22, 37, 56,  68, 109, 103,  77,
    24, 35, 55, 64,  81, 104, 113,  92,
    49, 64, 78, 87, 103, 121, 120, 101,
    72, 92, 95, 98, 112, 100, 103,  99,
];

#[rustfmt::skip]
const STD_CHROMA_QT: [u8; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99,
    18, 21, 26, 66, 99, 99, 99, 99,
    24, 26, 56, 99, 99, 99, 99, 99,
    47, 66, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
];

const DC_LUMA_BITS: [u8; 16] = [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
const DC_LUMA_VALS: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

const DC_CHROMA_BITS: [u8; 16] = [0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0];
const DC_CHROMA_VALS: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

const AC_LUMA_BITS: [u8; 16] = [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 125];
#[rustfmt::skip]
const AC_LUMA_VALS: [u8; 162] = [
    0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12,
    0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07,
    0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08,
    0x23, 0x42, 0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0,
    0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16,
    0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28,
    0x29, 0x2A, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39,
    0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49,
    0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59,
    0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
    0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79,
    0x7A, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
    0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98,
    0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7,
    0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6,
    0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5,
    0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4,
    0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2,
    0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA,
    0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8,
    0xF9, 0xFA,
];

const AC_CHROMA_BITS: [u8; 16] = [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 119];
#[rustfmt::skip]
const AC_CHROMA_VALS: [u8; 162] = [
    0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21,
    0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71,
    0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91,
    0xA1, 0xB1, 0xC1, 0x09, 0x23, 0x33, 0x52, 0xF0,
    0x15, 0x62, 0x72, 0xD1, 0x0A, 0x16, 0x24, 0x34,
    0xE1, 0x25, 0xF1, 0x17, 0x18, 0x19, 0x1A, 0x26,
    0x27, 0x28, 0x29, 0x2A, 0x35, 0x36, 0x37, 0x38,
    0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48,
    0x49, 0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58,
    0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68,
    0x69, 0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78,
    0x79, 0x7A, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
    0x88, 0x89, 0x8A, 0x92, 0x93, 0x94, 0x95, 0x96,
    0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5,
    0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4,
    0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3,
    0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2,
    0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA,
    0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9,
    0xEA, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8,
    0xF9, 0xFA,
];

struct HuffmanTables {
    dc_luma: [(u16, u8); 12],
    dc_chroma: [(u16, u8); 12],
    ac_luma: [(u16, u8); 256],
    ac_chroma: [(u16, u8); 256],
}

static HUFFMAN_TABLES: LazyLock<HuffmanTables> = LazyLock::new(|| HuffmanTables {
    dc_luma: build_huffman_lut::<12>(&DC_LUMA_BITS, &DC_LUMA_VALS),
    dc_chroma: build_huffman_lut::<12>(&DC_CHROMA_BITS, &DC_CHROMA_VALS),
    ac_luma: build_huffman_lut::<256>(&AC_LUMA_BITS, &AC_LUMA_VALS),
    ac_chroma: build_huffman_lut::<256>(&AC_CHROMA_BITS, &AC_CHROMA_VALS),
});

static DCT_BASIS: LazyLock<[[f32; 8]; 8]> = LazyLock::new(|| {
    let mut table = [[0.0_f32; 8]; 8];
    let inv_sqrt2 = std::f64::consts::FRAC_1_SQRT_2;
    let pi = std::f64::consts::PI;
    for (u, row) in table.iter_mut().enumerate() {
        let cu = if u == 0 { inv_sqrt2 } else { 1.0 };
        for (x, cell) in row.iter_mut().enumerate() {
            let angle = ((2 * x + 1) as f64 * u as f64 * pi) / 16.0;
            *cell = (0.5 * cu * angle.cos()) as f32;
        }
    }
    table
});

fn build_huffman_lut<const N: usize>(bits: &[u8; 16], vals: &[u8]) -> [(u16, u8); N] {
    let mut lut = [(0_u16, 0_u8); N];
    let mut code: u16 = 0;
    let mut idx = 0_usize;
    for (len_idx, &count) in bits.iter().enumerate() {
        let bit_len = (len_idx + 1) as u8;
        for _ in 0..count {
            let sym = vals[idx] as usize;
            if sym < N {
                lut[sym] = (code, bit_len);
            }
            code = code.wrapping_add(1);
            idx += 1;
        }
        code <<= 1;
    }
    lut
}

fn scale_quant_table(base: &[u8; 64], quality: u8) -> ([u8; 64], [f32; 64]) {
    let q = u32::from(quality.clamp(1, 100));
    let scale = if q < 50 { 5000 / q } else { 200 - 2 * q };
    let mut zigzag_qt = [0_u8; 64];
    let mut recip_natural = [0.0_f32; 64];
    for (zz_idx, &nat_idx) in ZIGZAG.iter().enumerate() {
        let raw = (u32::from(base[nat_idx]) * scale + 50) / 100;
        let clamped = raw.clamp(1, 255) as u8;
        zigzag_qt[zz_idx] = clamped;
        recip_natural[nat_idx] = 1.0 / f32::from(clamped);
    }
    (zigzag_qt, recip_natural)
}

struct BitWriter<'a> {
    out: &'a mut Vec<u8>,
    acc: u32,
    bits: u8,
}

impl<'a> BitWriter<'a> {
    fn new(out: &'a mut Vec<u8>) -> Self {
        Self {
            out,
            acc: 0,
            bits: 0,
        }
    }

    #[inline]
    fn write_bits(&mut self, value: u16, len: u8) {
        if len == 0 {
            return;
        }
        let mask = (1_u32 << len) - 1;
        self.acc = (self.acc << len) | (u32::from(value) & mask);
        self.bits += len;
        while self.bits >= 8 {
            self.bits -= 8;
            let byte = ((self.acc >> self.bits) & 0xFF) as u8;
            self.out.push(byte);
            if byte == 0xFF {
                self.out.push(0x00);
            }
        }
        if self.bits == 0 {
            self.acc = 0;
        } else {
            self.acc &= (1_u32 << self.bits) - 1;
        }
    }

    fn finish(mut self) {
        if self.bits > 0 {
            let pad = 8 - self.bits;
            self.write_bits((1_u16 << pad) - 1, pad);
        }
    }
}

#[inline]
fn encode_magnitude(val: i32) -> (u8, u16) {
    if val == 0 {
        return (0, 0);
    }
    let abs_val = val.unsigned_abs();
    let cat = (32 - abs_val.leading_zeros()) as u8;
    let bits = if val > 0 {
        abs_val as u16
    } else {
        ((1_u32 << cat) - 1 - abs_val) as u16
    };
    (cat, bits)
}

#[inline]
fn forward_dct_quantize(
    block: &[f32; 64],
    recip_qt: &[f32; 64],
    basis: &[[f32; 8]; 8],
) -> [i32; 64] {
    let mut tmp = [0.0_f32; 64];
    for y in 0..8 {
        let row = &block[y * 8..y * 8 + 8];
        for u in 0..8 {
            let bu = &basis[u];
            tmp[y * 8 + u] = row[0] * bu[0]
                + row[1] * bu[1]
                + row[2] * bu[2]
                + row[3] * bu[3]
                + row[4] * bu[4]
                + row[5] * bu[5]
                + row[6] * bu[6]
                + row[7] * bu[7];
        }
    }

    let mut coeffs = [0_i32; 64];
    for (v, bv) in basis.iter().enumerate() {
        for u in 0..8 {
            let sum = tmp[u] * bv[0]
                + tmp[8 + u] * bv[1]
                + tmp[16 + u] * bv[2]
                + tmp[24 + u] * bv[3]
                + tmp[32 + u] * bv[4]
                + tmp[40 + u] * bv[5]
                + tmp[48 + u] * bv[6]
                + tmp[56 + u] * bv[7];
            let idx = v * 8 + u;
            #[allow(clippy::cast_possible_truncation)]
            let q = (sum * recip_qt[idx]).round() as i32;
            coeffs[idx] = q.clamp(-1023, 1023);
        }
    }
    coeffs
}

fn encode_block(
    writer: &mut BitWriter<'_>,
    block: &[f32; 64],
    recip_qt: &[f32; 64],
    basis: &[[f32; 8]; 8],
    prev_dc: &mut i32,
    dc_lut: &[(u16, u8); 12],
    ac_lut: &[(u16, u8); 256],
) {
    let coeffs = forward_dct_quantize(block, recip_qt, basis);
    let dc = coeffs[0];
    let diff = (dc - *prev_dc).clamp(-2047, 2047);
    *prev_dc = dc;

    let (dc_cat, dc_bits) = encode_magnitude(diff);
    let (dc_code, dc_len) = dc_lut[dc_cat as usize];
    writer.write_bits(dc_code, dc_len);
    if dc_cat > 0 {
        writer.write_bits(dc_bits, dc_cat);
    }

    let mut zero_run = 0_u8;
    for &nat_idx in &ZIGZAG[1..] {
        let coeff = coeffs[nat_idx];
        if coeff == 0 {
            zero_run += 1;
        } else {
            while zero_run >= 16 {
                let (zrl_code, zrl_len) = ac_lut[0xF0];
                writer.write_bits(zrl_code, zrl_len);
                zero_run -= 16;
            }
            let (ac_cat, ac_bits) = encode_magnitude(coeff);
            let sym = ((zero_run << 4) | ac_cat) as usize;
            let (ac_code, ac_len) = ac_lut[sym];
            writer.write_bits(ac_code, ac_len);
            writer.write_bits(ac_bits, ac_cat);
            zero_run = 0;
        }
    }
    if zero_run > 0 {
        let (eob_code, eob_len) = ac_lut[0x00];
        writer.write_bits(eob_code, eob_len);
    }
}

fn write_u16_be(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn write_app0_jfif(out: &mut Vec<u8>) {
    out.extend_from_slice(&[
        0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01, 0x01, 0x00, 0x00, 0x01, 0x00,
        0x01, 0x00, 0x00,
    ]);
}

fn write_dqt(out: &mut Vec<u8>, table_id: u8, zigzag_qt: &[u8; 64]) {
    out.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x43, table_id]);
    out.extend_from_slice(zigzag_qt);
}

fn write_dht(out: &mut Vec<u8>, class_and_id: u8, bits: &[u8; 16], vals: &[u8]) {
    let Ok(len) = u16::try_from(3 + 16 + vals.len()) else {
        return;
    };
    out.extend_from_slice(&[0xFF, 0xC4]);
    write_u16_be(out, len);
    out.push(class_and_id);
    out.extend_from_slice(bits);
    out.extend_from_slice(vals);
}

fn validate_dimensions(width: u32, height: u32) -> Result<(u16, u16), JpegEncodeError> {
    if width == 0 || height == 0 {
        return Err(JpegEncodeError::ZeroDimensions);
    }
    let w = u16::try_from(width)
        .map_err(|_| JpegEncodeError::DimensionsExceedJpegLimit { width, height })?;
    let h = u16::try_from(height)
        .map_err(|_| JpegEncodeError::DimensionsExceedJpegLimit { width, height })?;
    Ok((w, h))
}

/// Encodes tightly packed RGB24 pixels (`width * height * 3` bytes) into a
/// baseline JFIF JPEG stream using YCbCr 4:2:0 subsampling.
///
/// # Errors
///
/// Returns [`JpegEncodeError`] when dimensions are zero or exceed `65535`,
/// or when `rgb.len()` does not equal `width * height * 3`.
pub fn encode_rgb_jpeg(
    rgb: &[u8],
    width: u32,
    height: u32,
    quality: u8,
) -> Result<Vec<u8>, JpegEncodeError> {
    let (w_u16, h_u16) = validate_dimensions(width, height)?;
    let w = width as usize;
    let h = height as usize;
    let expected = w
        .checked_mul(h)
        .and_then(|px| px.checked_mul(3))
        .ok_or(JpegEncodeError::DimensionsExceedJpegLimit { width, height })?;
    if rgb.len() != expected {
        return Err(JpegEncodeError::InvalidBufferLength {
            expected,
            actual: rgb.len(),
        });
    }

    let (luma_zz, luma_recip) = scale_quant_table(&STD_LUMA_QT, quality);
    let (chroma_zz, chroma_recip) = scale_quant_table(&STD_CHROMA_QT, quality);
    let huff = &*HUFFMAN_TABLES;
    let basis = &*DCT_BASIS;

    let mut out = Vec::with_capacity((expected / 6).max(1024));
    out.extend_from_slice(&[0xFF, 0xD8]); // SOI
    write_app0_jfif(&mut out);
    write_dqt(&mut out, 0x00, &luma_zz);
    write_dqt(&mut out, 0x01, &chroma_zz);

    // SOF0 (Baseline DCT, 3 components, 4:2:0)
    out.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
    write_u16_be(&mut out, h_u16);
    write_u16_be(&mut out, w_u16);
    out.extend_from_slice(&[
        0x03, // Nf = 3
        0x01, 0x22, 0x00, // Y: 2x2, QT 0
        0x02, 0x11, 0x01, // Cb: 1x1, QT 1
        0x03, 0x11, 0x01, // Cr: 1x1, QT 1
    ]);

    write_dht(&mut out, 0x00, &DC_LUMA_BITS, &DC_LUMA_VALS);
    write_dht(&mut out, 0x10, &AC_LUMA_BITS, &AC_LUMA_VALS);
    write_dht(&mut out, 0x01, &DC_CHROMA_BITS, &DC_CHROMA_VALS);
    write_dht(&mut out, 0x11, &AC_CHROMA_BITS, &AC_CHROMA_VALS);

    // SOS
    out.extend_from_slice(&[
        0xFF, 0xDA, 0x00, 0x0C, 0x03, 0x01, 0x00, 0x02, 0x11, 0x03, 0x11, 0x00, 0x3F, 0x00,
    ]);

    let mut writer = BitWriter::new(&mut out);
    let mut prev_dc_y = 0_i32;
    let mut prev_dc_cb = 0_i32;
    let mut prev_dc_cr = 0_i32;

    let mut y_mcu = [0.0_f32; 256];
    let mut cb_mcu = [0.0_f32; 256];
    let mut cr_mcu = [0.0_f32; 256];
    let mut y_block = [0.0_f32; 64];
    let mut cb_block = [0.0_f32; 64];
    let mut cr_block = [0.0_f32; 64];

    let mcu_cols = w.div_ceil(16);
    let mcu_rows = h.div_ceil(16);

    for mcu_y in 0..mcu_rows {
        let base_y = mcu_y * 16;
        for mcu_x in 0..mcu_cols {
            let base_x = mcu_x * 16;

            for dy in 0..16 {
                let sy = (base_y + dy).min(h - 1);
                let row_off = sy * w;
                for dx in 0..16 {
                    let sx = (base_x + dx).min(w - 1);
                    let px_off = (row_off + sx) * 3;
                    let r = f32::from(rgb[px_off]);
                    let g = f32::from(rgb[px_off + 1]);
                    let b = f32::from(rgb[px_off + 2]);
                    let mcu_idx = dy * 16 + dx;
                    y_mcu[mcu_idx] = 0.299 * r + 0.587 * g + 0.114 * b - 128.0;
                    cb_mcu[mcu_idx] = -0.168_736 * r - 0.331_264 * g + 0.5 * b;
                    cr_mcu[mcu_idx] = 0.5 * r - 0.418_688 * g - 0.081_312 * b;
                }
            }

            // 4 Y blocks in 2x2 grid: top-left, top-right, bottom-left, bottom-right
            for (sub_y, sub_x) in [(0, 0), (0, 8), (8, 0), (8, 8)] {
                for by in 0..8 {
                    let src_start = (sub_y + by) * 16 + sub_x;
                    y_block[by * 8..by * 8 + 8].copy_from_slice(&y_mcu[src_start..src_start + 8]);
                }
                encode_block(
                    &mut writer,
                    &y_block,
                    &luma_recip,
                    basis,
                    &mut prev_dc_y,
                    &huff.dc_luma,
                    &huff.ac_luma,
                );
            }

            // 4:2:0 subsampled Cb and Cr blocks (average of 2x2 pixel blocks)
            for by in 0..8 {
                let r0 = (by * 2) * 16;
                let r1 = r0 + 16;
                for bx in 0..8 {
                    let c0 = bx * 2;
                    let c1 = c0 + 1;
                    let out_idx = by * 8 + bx;
                    cb_block[out_idx] = 0.25
                        * (cb_mcu[r0 + c0] + cb_mcu[r0 + c1] + cb_mcu[r1 + c0] + cb_mcu[r1 + c1]);
                    cr_block[out_idx] = 0.25
                        * (cr_mcu[r0 + c0] + cr_mcu[r0 + c1] + cr_mcu[r1 + c0] + cr_mcu[r1 + c1]);
                }
            }

            encode_block(
                &mut writer,
                &cb_block,
                &chroma_recip,
                basis,
                &mut prev_dc_cb,
                &huff.dc_chroma,
                &huff.ac_chroma,
            );
            encode_block(
                &mut writer,
                &cr_block,
                &chroma_recip,
                basis,
                &mut prev_dc_cr,
                &huff.dc_chroma,
                &huff.ac_chroma,
            );
        }
    }

    writer.finish();
    out.extend_from_slice(&[0xFF, 0xD9]); // EOI
    Ok(out)
}

/// Encodes tightly packed 8-bit grayscale pixels (`width * height` bytes) into
/// a 1-component baseline JFIF JPEG stream.
///
/// # Errors
///
/// Returns [`JpegEncodeError`] when dimensions are zero or exceed `65535`,
/// or when `gray.len()` does not equal `width * height`.
pub fn encode_gray_jpeg(
    gray: &[u8],
    width: u32,
    height: u32,
    quality: u8,
) -> Result<Vec<u8>, JpegEncodeError> {
    let (w_u16, h_u16) = validate_dimensions(width, height)?;
    let w = width as usize;
    let h = height as usize;
    let expected = w
        .checked_mul(h)
        .ok_or(JpegEncodeError::DimensionsExceedJpegLimit { width, height })?;
    if gray.len() != expected {
        return Err(JpegEncodeError::InvalidBufferLength {
            expected,
            actual: gray.len(),
        });
    }

    let (luma_zz, luma_recip) = scale_quant_table(&STD_LUMA_QT, quality);
    let huff = &*HUFFMAN_TABLES;
    let basis = &*DCT_BASIS;

    let mut out = Vec::with_capacity((expected / 4).max(512));
    out.extend_from_slice(&[0xFF, 0xD8]); // SOI
    write_app0_jfif(&mut out);
    write_dqt(&mut out, 0x00, &luma_zz);

    // SOF0 (Baseline DCT, 1 component)
    out.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B, 0x08]);
    write_u16_be(&mut out, h_u16);
    write_u16_be(&mut out, w_u16);
    out.extend_from_slice(&[0x01, 0x01, 0x11, 0x00]);

    write_dht(&mut out, 0x00, &DC_LUMA_BITS, &DC_LUMA_VALS);
    write_dht(&mut out, 0x10, &AC_LUMA_BITS, &AC_LUMA_VALS);

    // SOS
    out.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00]);

    let mut writer = BitWriter::new(&mut out);
    let mut prev_dc = 0_i32;
    let mut block = [0.0_f32; 64];

    let mcu_cols = w.div_ceil(8);
    let mcu_rows = h.div_ceil(8);

    for mcu_y in 0..mcu_rows {
        let base_y = mcu_y * 8;
        for mcu_x in 0..mcu_cols {
            let base_x = mcu_x * 8;
            for by in 0..8 {
                let sy = (base_y + by).min(h - 1);
                let row_off = sy * w;
                for bx in 0..8 {
                    let sx = (base_x + bx).min(w - 1);
                    block[by * 8 + bx] = f32::from(gray[row_off + sx]) - 128.0;
                }
            }
            encode_block(
                &mut writer,
                &block,
                &luma_recip,
                basis,
                &mut prev_dc,
                &huff.dc_luma,
                &huff.ac_luma,
            );
        }
    }

    writer.finish();
    out.extend_from_slice(&[0xFF, 0xD9]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn huffman_tables_cover_all_standard_symbols() {
        assert_eq!(
            DC_LUMA_BITS.iter().map(|&b| usize::from(b)).sum::<usize>(),
            DC_LUMA_VALS.len()
        );
        assert_eq!(
            DC_CHROMA_BITS
                .iter()
                .map(|&b| usize::from(b))
                .sum::<usize>(),
            DC_CHROMA_VALS.len()
        );
        assert_eq!(
            AC_LUMA_BITS.iter().map(|&b| usize::from(b)).sum::<usize>(),
            AC_LUMA_VALS.len()
        );
        assert_eq!(
            AC_CHROMA_BITS
                .iter()
                .map(|&b| usize::from(b))
                .sum::<usize>(),
            AC_CHROMA_VALS.len()
        );

        let huff = &*HUFFMAN_TABLES;
        for cat in 0..12 {
            assert!(huff.dc_luma[cat].1 > 0);
            assert!(huff.dc_chroma[cat].1 > 0);
        }
        assert!(huff.ac_luma[0x00].1 > 0);
        assert!(huff.ac_luma[0xF0].1 > 0);
        assert!(huff.ac_chroma[0x00].1 > 0);
        assert!(huff.ac_chroma[0xF0].1 > 0);
        for run in 0..16_usize {
            for size in 1..=10_usize {
                let sym = (run << 4) | size;
                assert!(huff.ac_luma[sym].1 > 0, "missing AC luma sym {sym:#04x}");
                assert!(
                    huff.ac_chroma[sym].1 > 0,
                    "missing AC chroma sym {sym:#04x}"
                );
            }
        }
    }

    #[test]
    fn encode_rgb_handles_non_multiple_of_16_and_markers() {
        for (w, h) in [(1, 1), (7, 9), (16, 16), (33, 25)] {
            let mut rgb = Vec::with_capacity(w * h * 3);
            for y in 0..h {
                for x in 0..w {
                    rgb.push(((x * 13 + y * 7) % 256) as u8);
                    rgb.push(((x * 5 + y * 19) % 256) as u8);
                    rgb.push(((x * 23 + y * 3) % 256) as u8);
                }
            }
            let jpeg = encode_rgb_jpeg(&rgb, w as u32, h as u32, 75).expect("encode RGB");
            assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
            assert_eq!(&jpeg[jpeg.len() - 2..], &[0xFF, 0xD9]);
            assert!(jpeg.windows(4).any(|w| w == b"JFIF"));
        }
    }

    #[test]
    fn encode_gray_produces_valid_jfif_boundaries_and_quality_scales_size() {
        let w = 64_u32;
        let h = 64_u32;
        let mut gray = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                gray.push((((x * 3) ^ (y * 5)) & 0xFF) as u8);
            }
        }
        let q85 = encode_gray_jpeg(&gray, w, h, 85).expect("encode gray q85");
        let q50 = encode_gray_jpeg(&gray, w, h, 50).expect("encode gray q50");
        assert_eq!(&q85[..2], &[0xFF, 0xD8]);
        assert_eq!(&q85[q85.len() - 2..], &[0xFF, 0xD9]);
        assert!(q50.len() < q85.len());
    }

    #[test]
    fn encode_rejects_invalid_inputs() {
        assert!(matches!(
            encode_rgb_jpeg(&[], 0, 10, 75),
            Err(JpegEncodeError::ZeroDimensions)
        ));
        assert!(matches!(
            encode_rgb_jpeg(&[0; 10], 2, 2, 75),
            Err(JpegEncodeError::InvalidBufferLength {
                expected: 12,
                actual: 10
            })
        ));
        assert!(matches!(
            encode_gray_jpeg(&[0; 3], 2, 2, 75),
            Err(JpegEncodeError::InvalidBufferLength {
                expected: 4,
                actual: 3
            })
        ));
    }
}
