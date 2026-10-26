/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
// A decoder for MPEG-4 Part 2 video, Simple Profile (what iPhone OS apps
// of the 2.x/3.x era used for their movies, wrapped in mp4 files).
//
// Supported: I and P frames, 4 motion vectors per macroblock, unrestricted
// motion vectors, AC/DC prediction, video packets (resync markers) and
// skipped frames. Not supported: B frames, interlacing, sprites, quarter
// pixel motion, MPEG quantization matrices, data partitioning and the other
// tools that are not part of Simple Profile.
//
// The IDCT meets the accuracy criteria of IEEE Std 1180-1990 (see its tests),
// which is what the standard requires; other decoders may therefore differ
// from this one by one level in some pixels.
// It never panics on malformed input: errors are returned instead.

use super::mpeg4_tables::*;

/// Why a frame could not be decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The stream uses a feature this decoder doesn't implement.
    Unsupported(&'static str),
    /// The stream is malformed.
    Invalid(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

macro_rules! invalid {
    ($msg:expr) => {
        return Err(Error::Invalid($msg))
    };
}

macro_rules! unsupported {
    ($msg:expr) => {
        return Err(Error::Unsupported($msg))
    };
}

/// The largest picture dimension that will be accepted.
const MAX_DIMENSION: usize = 2048;

// ---------------------------------------------------------------------------
// Bit reader and variable length code lookup
// ---------------------------------------------------------------------------

/// Reads bits MSB-first. Reading past the end yields zero bits.
#[derive(Clone, Copy)]
struct Bits<'a> {
    data: &'a [u8],
    /// Position in bits.
    pos: usize,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Bits<'a> {
        Bits { data, pos: 0 }
    }

    /// Look at the next n (at most 32) bits without consuming them.
    fn peek(&self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let byte = self.pos >> 3;
        let mut v: u64 = 0;
        for i in 0..5 {
            let b = self.data.get(byte + i).copied().unwrap_or(0);
            v = (v << 8) | u64::from(b);
        }
        let shift = 40 - (self.pos & 7) as u32 - n;
        ((v >> shift) & ((1u64 << n) - 1)) as u32
    }

    fn skip(&mut self, n: u32) {
        self.pos += n as usize;
    }

    fn get(&mut self, n: u32) -> u32 {
        let v = self.peek(n);
        self.pos += n as usize;
        v
    }

    fn get1(&mut self) -> bool {
        self.get(1) != 0
    }
}

/// A lookup table for a prefix code.
struct Vlc {
    bits: u32,
    /// For every possible value of the next `bits` bits: the length of the
    /// code that matches (0 if none) and the symbol (its index).
    table: Vec<(u8, i16)>,
}

impl Vlc {
    fn new(bits: u32, codes: &[(u32, u32)]) -> Vlc {
        let mut table = vec![(0u8, 0i16); 1 << bits];
        for (sym, &(code, len)) in codes.iter().enumerate() {
            if len == 0 || len > bits {
                continue;
            }
            let base = (code << (bits - len)) as usize;
            for entry in table.iter_mut().skip(base).take(1 << (bits - len)) {
                *entry = (len as u8, sym as i16);
            }
        }
        Vlc { bits, table }
    }

    /// The lookup table for a run/level code table. The symbol of a code is
    /// the index of its entry, and the escape code gets the symbol after the
    /// last entry.
    fn from_tcoef(bits: u32, entries: &[Tcoef]) -> Vlc {
        let mut codes: Vec<(u32, u32)> = entries
            .iter()
            .map(|e| (u32::from(e.code), u32::from(e.len)))
            .collect();
        codes.push((u32::from(TCOEF_ESCAPE.0), u32::from(TCOEF_ESCAPE.1)));
        Vlc::new(bits, &codes)
    }

    fn from8(bits: u32, codes: &[(u8, u8)]) -> Vlc {
        let codes: Vec<(u32, u32)> = codes
            .iter()
            .map(|&(c, l)| (u32::from(c), u32::from(l)))
            .collect();
        Vlc::new(bits, &codes)
    }

    /// Decode a symbol, or None if the bits don't match any code.
    fn get(&self, b: &mut Bits) -> Option<usize> {
        let (len, sym) = self.table[b.peek(self.bits) as usize];
        if len == 0 {
            return None;
        }
        b.skip(u32::from(len));
        Some(sym as usize)
    }
}

/// Find the position of the next 00 00 01 start code prefix.
fn find_start_code(data: &[u8], from: usize) -> Option<usize> {
    let rest = data.get(from..)?;
    let offset = rest.windows(3).position(|w| w == [0, 0, 1])?;
    Some(from + offset)
}

// ---------------------------------------------------------------------------
// Inverse DCT, written directly from its definition (ISO/IEC 14496-2 Annex A,
// whose accuracy requirements are those of IEEE Std 1180-1990).
//
// The 2D transform is separable, so it is done as 1D transforms of every row
// and then of every column. A 1D transform of 8 values is
//
//     x[n] = sum over k of  a(k) * X[k] * cos((2n + 1) * k * pi / 16)
//
// with a(0) = 1 / (2 * sqrt(2)) and a(k) = 1 / 2 for the others. That makes
// each pass an orthonormal transform, so the two passes together have the
// factor of 1/4 of the 2D definition. Each coefficient of the basis matrix
// below is round(a(k) * cos(...) * 2^14), indexed [n][k].
//
// Between the passes a few extra fractional bits are kept, because rounding
// to whole numbers there would cost too much accuracy to meet IEEE 1180.
// 64-bit integers are used so that garbage input can't cause overflow panics.
// ---------------------------------------------------------------------------

const IDCT_BASIS_BITS: u32 = 14;
const IDCT_EXTRA_BITS: u32 = 5;

const IDCT_BASIS: [[i64; 8]; 8] = [
    [5793, 8035, 7568, 6811, 5793, 4551, 3135, 1598],
    [5793, 6811, 3135, -1598, -5793, -8035, -7568, -4551],
    [5793, 4551, -3135, -8035, -5793, 1598, 7568, 6811],
    [5793, 1598, -7568, -4551, 5793, 6811, -3135, -8035],
    [5793, -1598, -7568, 4551, 5793, -6811, -3135, 8035],
    [5793, -4551, -3135, 8035, -5793, -1598, 7568, -6811],
    [5793, -6811, 3135, 1598, -5793, 8035, -7568, 4551],
    [5793, -8035, 7568, -6811, 5793, -4551, 3135, -1598],
];

/// One 1D inverse transform: multiplies the basis matrix with `input`, then
/// divides by 2^`shift` with rounding.
///
/// The basis matrix is symmetric: row 7 - n is row n with the signs of the
/// entries in the odd numbered columns flipped. So the outputs n and 7 - n
/// (for n from 0 to 3) are the sum and the difference of two parts: the one
/// from the even numbered coefficients and the one from the odd numbered
/// ones. The even part has a symmetry of its own (row 3 - n is row n with the
/// signs of the columns 2 and 6 flipped), so it too is computed as a sum and
/// a difference. This gives exactly the same integers as the plain product,
/// with 24 multiplications instead of 64 (8 if the odd numbered coefficients
/// are all zero).
fn idct_pass(input: [i64; 8], shift: u32) -> [i64; 8] {
    if input == [0; 8] {
        return [0; 8];
    }
    let half = 1i64 << (shift - 1);
    let basis = &IDCT_BASIS;
    let mut even = [0i64; 4];
    for n in 0..2 {
        let from_0_and_4 = basis[n][0] * input[0] + basis[n][4] * input[4];
        let from_2_and_6 = basis[n][2] * input[2] + basis[n][6] * input[6];
        even[n] = from_0_and_4 + from_2_and_6;
        even[3 - n] = from_0_and_4 - from_2_and_6;
    }
    let mut odd = [0i64; 4];
    if (input[1] | input[3] | input[5] | input[7]) != 0 {
        for n in 0..4 {
            odd[n] = basis[n][1] * input[1]
                + basis[n][3] * input[3]
                + basis[n][5] * input[5]
                + basis[n][7] * input[7];
        }
    }
    let mut out = [0i64; 8];
    for n in 0..4 {
        out[n] = (even[n] + odd[n] + half) >> shift;
        out[7 - n] = (even[n] - odd[n] + half) >> shift;
    }
    out
}

/// The inverse DCT of a block of 64 coefficients (in raster order). The
/// result is indexed `[row][column]` and has not been clipped.
fn idct_8x8(blk: &[i16; 64]) -> [[i64; 8]; 8] {
    let mut rows = [[0i64; 8]; 8];
    for (y, row) in rows.iter_mut().enumerate() {
        let mut coeffs = [0i64; 8];
        for (k, c) in coeffs.iter_mut().enumerate() {
            *c = i64::from(blk[y * 8 + k]);
        }
        *row = idct_pass(coeffs, IDCT_BASIS_BITS - IDCT_EXTRA_BITS);
    }
    let mut result = [[0i64; 8]; 8];
    for x in 0..8 {
        let mut column = [0i64; 8];
        for (k, c) in column.iter_mut().enumerate() {
            *c = rows[k][x];
        }
        let values = idct_pass(column, IDCT_BASIS_BITS + IDCT_EXTRA_BITS);
        for (y, &v) in values.iter().enumerate() {
            result[y][x] = v;
        }
    }
    result
}

/// Do the IDCT of a block and either store the result to or add it to the
/// 8x8 area starting at `offset` in `dst`.
fn idct_store(blk: &[i16; 64], dst: &mut [u8], offset: usize, stride: usize, add: bool) {
    let result = idct_8x8(blk);
    for (y, row) in result.iter().enumerate() {
        for (x, &value) in row.iter().enumerate() {
            let i = offset + y * stride + x;
            let mut v = value;
            if add {
                v += i64::from(dst[i]);
            }
            dst[i] = v.clamp(0, 255) as u8;
        }
    }
}

#[cfg(test)]
mod idct_tests {
    use super::{idct_8x8, idct_pass, IDCT_BASIS};

    /// Orthonormal 8 point DCT basis in floating point, indexed `[n][k]`.
    fn basis() -> [[f64; 8]; 8] {
        let mut c = [[0.0f64; 8]; 8];
        for n in 0..8 {
            for k in 0..8 {
                let a = if k == 0 { 0.5 / 2f64.sqrt() } else { 0.5 };
                let angle = (2 * n + 1) as f64 * k as f64 * std::f64::consts::PI / 16.0;
                c[n][k] = a * angle.cos();
            }
        }
        c
    }

    fn forward(c: &[[f64; 8]; 8], px: &[[f64; 8]; 8]) -> [[f64; 8]; 8] {
        let mut out = [[0.0f64; 8]; 8];
        for v in 0..8 {
            for u in 0..8 {
                let mut sum = 0.0;
                for y in 0..8 {
                    for x in 0..8 {
                        sum += c[y][v] * px[y][x] * c[x][u];
                    }
                }
                out[v][u] = sum;
            }
        }
        out
    }

    fn inverse(c: &[[f64; 8]; 8], f: &[[f64; 8]; 8]) -> [[f64; 8]; 8] {
        let mut out = [[0.0f64; 8]; 8];
        for y in 0..8 {
            for x in 0..8 {
                let mut sum = 0.0;
                for v in 0..8 {
                    for u in 0..8 {
                        sum += c[y][v] * f[v][u] * c[x][u];
                    }
                }
                out[y][x] = sum;
            }
        }
        out
    }

    struct Rng(u64);

    impl Rng {
        fn next_u64(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        /// A number from lo to hi, inclusive.
        fn range(&mut self, lo: i64, hi: i64) -> i64 {
            lo + (self.next_u64() % (hi - lo + 1) as u64) as i64
        }
    }

    /// The accuracy criteria of IEEE Std 1180-1990 for one range of input
    /// pixel values (-l to h, optionally negated), compared to a double
    /// precision IDCT. (The random numbers are not the ones of the standard.)
    fn check_range(l: i64, h: i64, negate: bool) {
        let c = basis();
        let mut rng = Rng(0x1180_1180_1180_1180);
        let blocks = 2000;
        let mut sum_err = [[0.0f64; 8]; 8];
        let mut sum_sq = [[0.0f64; 8]; 8];
        let mut peak = 0.0f64;
        for _ in 0..blocks {
            let mut px = [[0.0f64; 8]; 8];
            for row in px.iter_mut() {
                for p in row.iter_mut() {
                    let v = rng.range(-l, h);
                    *p = (if negate { -v } else { v }) as f64;
                }
            }
            let f = forward(&c, &px);
            let mut coeffs = [[0.0f64; 8]; 8];
            let mut blk = [0i16; 64];
            for v in 0..8 {
                for u in 0..8 {
                    let q = (f[v][u] + 0.5).floor().clamp(-2048.0, 2047.0);
                    coeffs[v][u] = q;
                    blk[v * 8 + u] = q as i16;
                }
            }
            let expected = inverse(&c, &coeffs);
            let got = idct_8x8(&blk);
            for y in 0..8 {
                for x in 0..8 {
                    let e = (expected[y][x] + 0.5).floor().clamp(-256.0, 255.0);
                    let g = got[y][x].clamp(-256, 255) as f64;
                    let err = g - e;
                    peak = peak.max(err.abs());
                    sum_err[y][x] += err;
                    sum_sq[y][x] += err * err;
                }
            }
        }
        let n = f64::from(blocks);
        let mut worst_pixel_mse = 0.0f64;
        let mut worst_pixel_mean = 0.0f64;
        let mut total_mse = 0.0;
        let mut total_mean = 0.0;
        for y in 0..8 {
            for x in 0..8 {
                worst_pixel_mse = worst_pixel_mse.max(sum_sq[y][x] / n);
                worst_pixel_mean = worst_pixel_mean.max((sum_err[y][x] / n).abs());
                total_mse += sum_sq[y][x] / n / 64.0;
                total_mean += sum_err[y][x] / n / 64.0;
            }
        }
        let case = format!("range {}..{}, negated: {}", -l, h, negate);
        assert!(peak <= 1.0, "peak error {} ({})", peak, case);
        assert!(worst_pixel_mse <= 0.06, "pixel MSE {} ({})", worst_pixel_mse, case);
        assert!(total_mse <= 0.02, "overall MSE {} ({})", total_mse, case);
        assert!(worst_pixel_mean <= 0.015, "pixel mean {} ({})", worst_pixel_mean, case);
        assert!(total_mean.abs() <= 0.0015, "overall mean {} ({})", total_mean, case);
    }

    #[test]
    fn meets_the_ieee_1180_accuracy_criteria() {
        for (l, h) in [(256, 255), (5, 5), (300, 300)] {
            check_range(l, h, false);
            check_range(l, h, true);
        }
    }

    /// The pass uses the symmetry of the basis matrix; it has to give the same
    /// numbers as the plain product, also for the extremes and for rows that
    /// are mostly zero, like those of real blocks.
    #[test]
    fn pass_equals_the_plain_matrix_product() {
        let mut rng = Rng(0x2545_f491_4f6c_dd1d);
        for i in 0..20000 {
            let mut input = [0i64; 8];
            for v in input.iter_mut() {
                *v = match i % 3 {
                    0 => rng.range(-32768, 32767),
                    1 => [-32768, 0, 32767][rng.range(0, 2) as usize],
                    _ if rng.range(0, 3) == 0 => rng.range(-2048, 2047),
                    _ => 0,
                };
            }
            for shift in [9u32, 19] {
                let half = 1i64 << (shift - 1);
                let mut expected = [0i64; 8];
                for n in 0..8 {
                    let sum: i64 = (0..8).map(|k| IDCT_BASIS[n][k] * input[k]).sum();
                    expected[n] = (sum + half) >> shift;
                }
                assert_eq!(idct_pass(input, shift), expected, "{:?}", input);
            }
        }
    }

    #[test]
    fn all_zero_input_gives_all_zero_output() {
        assert_eq!(idct_8x8(&[0; 64]), [[0; 8]; 8]);
    }
}

// ---------------------------------------------------------------------------
// Decoder
// ---------------------------------------------------------------------------

fn y_dc_scale(q: i32) -> i32 {
    if q <= 4 {
        8
    } else if q <= 8 {
        2 * q
    } else if q <= 24 {
        q + 8
    } else {
        2 * q - 16
    }
}

fn c_dc_scale(q: i32) -> i32 {
    if q <= 4 {
        8
    } else if q <= 24 {
        (q + 13) / 2
    } else {
        q - 6
    }
}

/// Inverse quantization of a non-zero coefficient level with quantizer `q`
/// (the H.263 style method, used when no quantization matrices are):
/// magnitude = q * (2 * |level| + 1), minus 1 if q is even, with the sign of
/// the level, limited to the range of 12 bit coefficients.
fn dequantize(level: i32, q: i32) -> i32 {
    let mut magnitude = q * (2 * level.abs() + 1);
    if q % 2 == 0 {
        magnitude -= 1;
    }
    let value = if level < 0 { -magnitude } else { magnitude };
    value.clamp(-2048, 2047)
}

/// The largest level of the entries of a run/level code table that have the
/// given last flag and run (0 if there are none). An escape sequence of the
/// first kind codes a level as the difference to this.
fn largest_level(entries: &[Tcoef], last: bool, run: u8) -> i32 {
    entries
        .iter()
        .filter(|e| e.last == last && e.run == run)
        .map(|e| i32::from(e.level))
        .max()
        .unwrap_or(0)
}

/// The largest run of the entries of a run/level code table that have the
/// given last flag and level (0 if there are none). An escape sequence of the
/// second kind codes a run as the difference to this, minus one.
fn largest_run(entries: &[Tcoef], last: bool, level: u8) -> i32 {
    entries
        .iter()
        .filter(|e| e.last == last && e.level == level)
        .map(|e| i32::from(e.run))
        .max()
        .unwrap_or(0)
}

fn median3(a: i32, b: i32, c: i32) -> i32 {
    let mx = a.max(b);
    let mn = a.min(b);
    c.min(mx).max(mn)
}

/// Gives the chroma motion vector (in half pixel units) for a macroblock
/// with four luma vectors, from the sum of the luma vectors (in half pixel
/// units of the luma plane). The average of the four, scaled down for the
/// half size chroma planes, is sum / 8 half pixels. Its fraction is rounded
/// to a whole or half pixel: the sixteenths 0 to 2 become a whole pixel
/// down, 3 to 13 a half pixel, and 14 and 15 a whole pixel up.
fn round_chroma(sum: i32) -> i32 {
    let whole_pixels = sum.div_euclid(16);
    let half_pixels = match sum.rem_euclid(16) {
        0..=2 => 0,
        3..=13 => 1,
        _ => 2,
    };
    whole_pixels * 2 + half_pixels
}

/// Gives one component of the chroma motion vector (in half pixel units) for
/// a macroblock with a single luma vector (in half pixel units of the luma
/// plane). The chroma planes are half the size, so the vector is halved. If
/// that leaves a quarter pixel fraction, the result is the half pixel
/// position within the same whole pixel.
fn chroma_vector_component(luma: i32) -> i32 {
    if luma % 2 == 0 {
        luma / 2
    } else {
        luma.div_euclid(4) * 2 + 1
    }
}

/// A decoded picture, in planar YUV 4:2:0.
pub struct Frame<'a> {
    pub width: usize,
    pub height: usize,
    pub y: &'a [u8],
    pub u: &'a [u8],
    pub v: &'a [u8],
    pub y_stride: usize,
    pub uv_stride: usize,
}

impl Frame<'_> {
    /// The color of a pixel as RGB (BT.601 limited range).
    fn rgb_at(&self, row: usize, col: usize) -> [u8; 3] {
        let y = i32::from(self.y[row * self.y_stride + col]) - 16;
        let uv_index = (row / 2) * self.uv_stride + col / 2;
        let u = i32::from(self.u[uv_index]) - 128;
        let v = i32::from(self.v[uv_index]) - 128;
        let c = 298 * y;
        let r = (c + 409 * v + 128) >> 8;
        let g = (c - 100 * u - 208 * v + 128) >> 8;
        let b = (c + 516 * u + 128) >> 8;
        [
            r.clamp(0, 255) as u8,
            g.clamp(0, 255) as u8,
            b.clamp(0, 255) as u8,
        ]
    }

    /// Convert to RGBA (8 bits per channel, BT.601 limited range) in `out`,
    /// which must have room for width x height x 4 bytes.
    pub fn write_rgba(&self, out: &mut [u8]) {
        for row in 0..self.height {
            for col in 0..self.width {
                let [r, g, b] = self.rgb_at(row, col);
                let o = (row * self.width + col) * 4;
                if let Some(px) = out.get_mut(o..o + 4) {
                    px.copy_from_slice(&[r, g, b, 255]);
                }
            }
        }
    }

    /// Like `write_rgba`, but the picture is rotated by 90 degrees, so the
    /// result is height pixels wide and width pixels tall.
    pub fn write_rgba_rotated(&self, out: &mut [u8], clockwise: bool) {
        for row in 0..self.height {
            for col in 0..self.width {
                let [r, g, b] = self.rgb_at(row, col);
                let (x, y) = if clockwise {
                    (self.height - 1 - row, col)
                } else {
                    (row, self.width - 1 - col)
                };
                let o = (y * self.height + x) * 4;
                if let Some(px) = out.get_mut(o..o + 4) {
                    px.copy_from_slice(&[r, g, b, 255]);
                }
            }
        }
    }
}

pub struct Decoder {
    width: usize,
    height: usize,
    mb_w: usize,
    mb_h: usize,
    time_bits: u32,
    resync_disable: bool,
    have_vol: bool,

    // State of the VOP being decoded.
    coding_type: u32,
    rounding: i32,
    fcode: u32,
    qscale: i32,
    dc_thr: u32,
    /// Index of the first macroblock of the current video packet.
    packet_start: usize,

    // Picture buffers, indexed by plane (Y, U, V).
    cur: [Vec<u8>; 3],
    reference: [Vec<u8>; 3],
    stride: [usize; 3],
    pw: [usize; 3],
    ph: [usize; 3],
    have_ref: bool,

    // Per macroblock: quantizer.
    qtab: Vec<u8>,
    // Motion vectors per 8x8 luma block, x and y interleaved.
    mvs: Vec<i16>,
    // DC values and AC prediction values per 8x8 block, for each plane.
    dcv: [Vec<i16>; 3],
    acv: [Vec<i16>; 3],
    // Size of each plane in blocks.
    bw: [usize; 3],
    bh: [usize; 3],

    v_inter: Vlc,
    v_intra: Vlc,
    v_mv: Vlc,
    v_dcl: Vlc,
    v_dcc: Vlc,
    v_cbpy: Vlc,
    v_mcbpc_i: Vlc,
    v_mcbpc_p: Vlc,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    pub fn new() -> Decoder {
        Decoder {
            width: 0,
            height: 0,
            mb_w: 0,
            mb_h: 0,
            time_bits: 1,
            resync_disable: true,
            have_vol: false,
            coding_type: 0,
            rounding: 0,
            fcode: 1,
            qscale: 1,
            dc_thr: 0,
            packet_start: 0,
            cur: [Vec::new(), Vec::new(), Vec::new()],
            reference: [Vec::new(), Vec::new(), Vec::new()],
            stride: [0; 3],
            pw: [0; 3],
            ph: [0; 3],
            have_ref: false,
            qtab: Vec::new(),
            mvs: Vec::new(),
            dcv: [Vec::new(), Vec::new(), Vec::new()],
            acv: [Vec::new(), Vec::new(), Vec::new()],
            bw: [0; 3],
            bh: [0; 3],
            v_inter: Vlc::from_tcoef(12, &TCOEF_INTER),
            v_intra: Vlc::from_tcoef(12, &TCOEF_INTRA),
            v_mv: Vlc::from8(12, &MV_VLC),
            v_dcl: Vlc::from8(12, &DC_LUM_VLC),
            v_dcc: Vlc::from8(12, &DC_CHROM_VLC),
            v_cbpy: Vlc::from8(6, &CBPY_VLC),
            v_mcbpc_i: Vlc::from8(9, &MCBPC_INTRA_VLC),
            v_mcbpc_p: Vlc::from8(9, &MCBPC_INTER_VLC),
        }
    }

    /// The size of the pictures, once the stream headers have been seen.
    pub fn dimensions(&self) -> Option<(usize, usize)> {
        if self.have_vol {
            Some((self.width, self.height))
        } else {
            None
        }
    }

    /// The most recently decoded picture.
    pub fn frame(&self) -> Option<Frame<'_>> {
        if !self.have_ref {
            return None;
        }
        Some(Frame {
            width: self.width,
            height: self.height,
            y: &self.reference[0],
            u: &self.reference[1],
            v: &self.reference[2],
            y_stride: self.stride[0],
            uv_stride: self.stride[1],
        })
    }

    /// Decode some data from the stream: it may contain stream headers (with
    /// start codes) and/or one frame. This is what each sample of an mp4
    /// video track is, and what the decoder configuration data of the track
    /// is. Returns whether a frame was decoded; if so, it can be retrieved
    /// with `frame`.
    pub fn decode_packet(&mut self, data: &[u8]) -> Result<bool> {
        let mut decoded = false;
        let mut from = 0;
        while let Some(start) = find_start_code(data, from) {
            if start + 4 > data.len() {
                break;
            }
            let code = data[start + 3];
            let end = find_start_code(data, start + 4).unwrap_or(data.len());
            let mut b = Bits::new(&data[start + 4..end]);
            match code {
                0x20..=0x2f => {
                    if !self.have_vol {
                        self.parse_vol(&mut b)?;
                        self.alloc_frame();
                    }
                }
                0xb6 => {
                    if !self.have_vol {
                        invalid!("frame before the stream headers");
                    }
                    if self.decode_vop(&mut b)? {
                        std::mem::swap(&mut self.cur, &mut self.reference);
                        self.have_ref = true;
                    }
                    decoded = true;
                }
                _ => {}
            }
            from = end;
        }
        Ok(decoded)
    }

    // -- headers --

    /// Parse a video object layer header (after its start code).
    fn parse_vol(&mut self, b: &mut Bits) -> Result<()> {
        b.skip(1); // random_accessible_vol
        b.skip(8); // video_object_type_indication
        let mut verid = 1;
        if b.get1() {
            verid = b.get(4);
            b.skip(3);
        }
        if b.get(4) == 15 {
            b.skip(16); // extended pixel aspect ratio
        }
        if b.get1() {
            // vol_control_parameters
            b.skip(3);
            if b.get1() {
                // vbv_parameters
                b.skip(79);
            }
        }
        if b.get(2) != 0 {
            unsupported!("non-rectangular video");
        }
        b.skip(1);
        let resolution = b.get(16);
        if resolution == 0 {
            invalid!("zero time increment resolution");
        }
        b.skip(1);
        self.time_bits = 32 - (resolution - 1).leading_zeros();
        self.time_bits = self.time_bits.max(1);
        if b.get1() {
            b.skip(self.time_bits); // fixed_vop_time_increment
        }
        b.skip(1);
        let width = b.get(13) as usize;
        b.skip(1);
        let height = b.get(13) as usize;
        b.skip(1);
        if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
            unsupported!("picture size");
        }
        if b.get1() {
            unsupported!("interlaced video");
        }
        b.skip(1); // obmc_disable
        let sprite = if verid == 1 { b.get(1) } else { b.get(2) };
        if sprite != 0 {
            unsupported!("sprites");
        }
        // not_8_bit: only the usual 5 bit quantizer and 8 bit pixels work.
        if b.get1() && (b.get(4) != 5 || b.get(4) != 8) {
            unsupported!("unusual quantizer precision");
        }
        if b.get1() {
            unsupported!("MPEG quantization matrices");
        }
        if verid != 1 && b.get1() {
            unsupported!("quarter pixel motion");
        }
        if !b.get1() {
            unsupported!("complexity estimation");
        }
        self.resync_disable = b.get1();
        if b.get1() {
            unsupported!("data partitioning");
        }
        if verid != 1 && (b.get1() || b.get1()) {
            unsupported!("newpred or reduced resolution");
        }
        if b.get1() {
            unsupported!("scalability");
        }
        self.width = width;
        self.height = height;
        self.mb_w = width.div_ceil(16);
        self.mb_h = height.div_ceil(16);
        self.have_vol = true;
        Ok(())
    }

    fn alloc_frame(&mut self) {
        for p in 0..3 {
            self.pw[p] = if p == 0 { self.mb_w * 16 } else { self.mb_w * 8 };
            self.ph[p] = if p == 0 { self.mb_h * 16 } else { self.mb_h * 8 };
            self.stride[p] = self.pw[p];
            self.cur[p] = vec![0; self.pw[p] * self.ph[p]];
            self.reference[p] = vec![0; self.pw[p] * self.ph[p]];
            self.bw[p] = if p == 0 { self.mb_w * 2 } else { self.mb_w };
            self.bh[p] = if p == 0 { self.mb_h * 2 } else { self.mb_h };
            self.dcv[p] = vec![1024; self.bw[p] * self.bh[p]];
            self.acv[p] = vec![0; self.bw[p] * self.bh[p] * 16];
        }
        self.qtab = vec![0; self.mb_w * self.mb_h];
        self.mvs = vec![0; self.mb_w * 2 * self.mb_h * 2 * 2];
    }

    /// Decode a video object plane (after its start code). Returns false if
    /// it is marked as not coded, meaning the previous picture is repeated.
    fn decode_vop(&mut self, b: &mut Bits) -> Result<bool> {
        self.coding_type = b.get(2);
        if self.coding_type > 1 {
            unsupported!("B frames and sprites");
        }
        while b.get1() {} // modulo_time_base
        b.skip(1);
        b.skip(self.time_bits); // vop_time_increment
        b.skip(1);
        if !b.get1() {
            return Ok(false); // vop_coded
        }
        self.rounding = 0;
        if self.coding_type == 1 {
            self.rounding = b.get(1) as i32;
        }
        self.dc_thr = b.get(3);
        self.qscale = b.get(5) as i32;
        if self.qscale == 0 {
            invalid!("zero quantizer");
        }
        self.fcode = 1;
        if self.coding_type == 1 {
            self.fcode = b.get(3);
            if self.fcode == 0 {
                invalid!("zero f_code");
            }
            if !self.have_ref {
                invalid!("predicted frame without a reference");
            }
        }
        for p in 0..3 {
            self.dcv[p].fill(1024);
            self.acv[p].fill(0);
        }
        self.mvs.fill(0);
        self.decode_macroblocks(b)?;
        Ok(true)
    }

    // -- prediction helpers --

    /// Whether a macroblock exists and is part of the current video packet,
    /// so that it can be used for prediction.
    fn mb_avail(&self, mbx: i32, mby: i32) -> bool {
        mbx >= 0
            && mby >= 0
            && (mbx as usize) < self.mb_w
            && (mby as usize) < self.mb_h
            && mby as usize * self.mb_w + mbx as usize >= self.packet_start
    }

    /// Predict the DC level of a block from its neighbors. Returns the
    /// prediction (in quantized units) and the direction: 0 for predicting
    /// from the left, 1 from the top.
    fn predict_dc(&self, plane: usize, bx: i32, by: i32, scale: i32) -> (i32, i32) {
        let sh = if plane == 0 { 1 } else { 0 };
        let pos = [(bx - 1, by), (bx - 1, by - 1), (bx, by - 1)];
        let mut vals = [1024i32; 3];
        for (k, &(x, y)) in pos.iter().enumerate() {
            if x >= 0 && y >= 0 && self.mb_avail(x >> sh, y >> sh) {
                let i = y as usize * self.bw[plane] + x as usize;
                vals[k] = i32::from(self.dcv[plane][i]);
            }
        }
        let (a, b, c) = (vals[0], vals[1], vals[2]);
        let (pred, dir) = if (a - b).abs() < (b - c).abs() {
            (c, 1)
        } else {
            (a, 0)
        };
        ((pred + (scale >> 1)) / scale, dir)
    }

    /// Mark the DC/AC prediction values of a macroblock as unavailable (used
    /// for macroblocks that aren't intra coded).
    fn reset_pred(&mut self, mbx: usize, mby: usize) {
        for p in 0..3 {
            let count = if p == 0 { 4 } else { 1 };
            for k in 0..count {
                let (bx, by) = if p == 0 {
                    (mbx * 2 + (k & 1), mby * 2 + (k >> 1))
                } else {
                    (mbx, mby)
                };
                let i = by * self.bw[p] + bx;
                self.dcv[p][i] = 1024;
                self.acv[p][i * 16..i * 16 + 16].fill(0);
            }
        }
    }

    fn set_mvs(&mut self, mbx: usize, mby: usize, mv: [(i32, i32); 4]) {
        let bw = self.mb_w * 2;
        for (k, &(x, y)) in mv.iter().enumerate() {
            let i = ((mby * 2 + (k >> 1)) * bw + mbx * 2 + (k & 1)) * 2;
            self.mvs[i] = x as i16;
            self.mvs[i + 1] = y as i16;
        }
    }

    /// Predict the motion vector of block n (in 0 to 3) of a macroblock.
    ///
    /// The prediction is the median of the vectors of three neighboring
    /// blocks, each given as the offset of a macroblock and the block within
    /// it (blocks are numbered in raster order within a macroblock): the
    /// block to the left, the one above, and the one above and to the right.
    /// A neighbor outside the picture or the current video packet counts as
    /// the zero vector, except that if it is the only one that is there, its
    /// vector is used as is.
    fn pred_mv(&self, mbx: usize, mby: usize, n: usize) -> (i32, i32) {
        const NEIGHBORS: [[(i32, i32, usize); 3]; 4] = [
            [(-1, 0, 1), (0, -1, 2), (1, -1, 2)],
            [(0, 0, 0), (0, -1, 3), (1, -1, 2)],
            [(-1, 0, 3), (0, 0, 0), (0, 0, 1)],
            [(0, 0, 2), (0, 0, 1), (0, 0, 0)],
        ];
        let blocks_per_row = self.mb_w * 2;
        let mut vectors = [(0i32, 0i32); 3];
        let mut present = 0;
        let mut only_one = (0, 0);
        for (slot, &(dx, dy, block)) in NEIGHBORS[n].iter().enumerate() {
            let (nx, ny) = (mbx as i32 + dx, mby as i32 + dy);
            if !self.mb_avail(nx, ny) {
                continue;
            }
            let row = ny as usize * 2 + (block >> 1);
            let col = nx as usize * 2 + (block & 1);
            let i = (row * blocks_per_row + col) * 2;
            vectors[slot] = (i32::from(self.mvs[i]), i32::from(self.mvs[i + 1]));
            only_one = vectors[slot];
            present += 1;
        }
        if present == 1 {
            return only_one;
        }
        (
            median3(vectors[0].0, vectors[1].0, vectors[2].0),
            median3(vectors[0].1, vectors[1].1, vectors[2].1),
        )
    }

    /// Decode one motion vector component.
    fn decode_mv_comp(&self, b: &mut Bits, pred: i32) -> Option<i32> {
        let code = self.v_mv.get(b)? as i32;
        if code == 0 {
            return Some(pred);
        }
        let negative = b.get1();
        let shift = self.fcode - 1;
        let mut val = code;
        if shift > 0 {
            val = ((val - 1) << shift) | b.get(shift) as i32;
            val += 1;
        }
        if negative {
            val = -val;
        }
        val += pred;
        // The result wraps around within the range allowed by f_code.
        let bits = 5 + self.fcode;
        val &= (1 << bits) - 1;
        if val >= (1 << (bits - 1)) {
            val -= 1 << bits;
        }
        Some(val)
    }

    // -- motion compensation --

    /// Predict a size x size block of plane p at (dx0, dy0) from the
    /// reference picture, with a motion vector in half pixel units.
    fn mc_block(&mut self, p: usize, dx0: usize, dy0: usize, mvx: i32, mvy: i32, size: usize) {
        let x = dx0 as i32 + (mvx >> 1);
        let y = dy0 as i32 + (mvy >> 1);
        let hx = (mvx & 1) != 0;
        let hy = (mvy & 1) != 0;
        let rnd = self.rounding;
        let stride = self.stride[p];
        let (max_x, max_y) = (self.pw[p] as i32 - 1, self.ph[p] as i32 - 1);
        let reference = &self.reference[p];
        let cur = &mut self.cur[p];
        // Pixels outside the picture are copies of the nearest edge pixel.
        let pix = |xx: i32, yy: i32| -> i32 {
            let i = yy.clamp(0, max_y) as usize * stride + xx.clamp(0, max_x) as usize;
            i32::from(reference[i])
        };
        for j in 0..size {
            for i in 0..size {
                let xi = x + i as i32;
                let yj = y + j as i32;
                let a = pix(xi, yj);
                let v = if !hx && !hy {
                    a
                } else if hx && !hy {
                    (a + pix(xi + 1, yj) + 1 - rnd) >> 1
                } else if !hx && hy {
                    (a + pix(xi, yj + 1) + 1 - rnd) >> 1
                } else {
                    let sum = a + pix(xi + 1, yj) + pix(xi, yj + 1) + pix(xi + 1, yj + 1);
                    (sum + 2 - rnd) >> 2
                };
                cur[(dy0 + j) * stride + dx0 + i] = v as u8;
            }
        }
    }

    // -- blocks --

    /// Decode the coefficients of one 8x8 block (n is 0 to 3 for luma, 4 and
    /// 5 for chroma). For intra blocks the result is the dequantized
    /// coefficients with DC and AC prediction applied; for inter blocks the
    /// dequantized residual.
    fn decode_block(
        &mut self,
        b: &mut Bits,
        blk: &mut [i16; 64],
        n: usize,
        intra: bool,
        coded: bool,
        ac_pred: bool,
        mbx: usize,
        mby: usize,
        qscale: i32,
    ) -> Result<()> {
        let plane = if n < 4 { 0 } else { n - 3 };
        let (bx, by) = if plane == 0 {
            (mbx * 2 + (n & 1), mby * 2 + (n >> 1))
        } else {
            (mbx, mby)
        };
        let bi = by * self.bw[plane] + bx;
        blk.fill(0);
        let dcscale = if plane == 0 {
            y_dc_scale(qscale)
        } else {
            c_dc_scale(qscale)
        };
        let mut pos: i32 = -1;
        let mut scan: &[u8; 64] = &ZIGZAG_SCAN;
        let mut dir = 0;
        if intra {
            let limit = match self.dc_thr {
                0 => 99,
                1 => 13,
                2 => 15,
                3 => 17,
                4 => 19,
                5 => 21,
                6 => 23,
                _ => 0,
            };
            if qscale >= limit {
                unsupported!("intra DC coded like AC coefficients");
            }
            let dc_vlc = if plane == 0 { &self.v_dcl } else { &self.v_dcc };
            let size = match dc_vlc.get(b) {
                Some(s) => s as u32,
                None => invalid!("bad DC size code"),
            };
            let mut diff = 0i32;
            if size > 0 {
                diff = b.get(size) as i32;
                if diff < (1 << (size - 1)) {
                    diff -= (1 << size) - 1;
                }
                if size > 8 {
                    b.skip(1); // marker bit
                }
            }
            let (pred, d) = self.predict_dc(plane, bx as i32, by as i32, dcscale);
            dir = d;
            let level = (pred + diff).max(0);
            self.dcv[plane][bi] = (level * dcscale) as i16;
            blk[0] = level as i16;
            pos = 0;
            if ac_pred {
                scan = if dir == 0 {
                    &ALT_VERTICAL_SCAN
                } else {
                    &ALT_HORIZONTAL_SCAN
                };
            }
        }
        if coded {
            let entries: &[Tcoef] = if intra { &TCOEF_INTRA } else { &TCOEF_INTER };
            let vlc = if intra { &self.v_intra } else { &self.v_inter };
            let escape = entries.len();
            loop {
                let idx = match vlc.get(b) {
                    Some(i) => i,
                    None => invalid!("bad coefficient code"),
                };
                let (last, run, level) = if idx == escape {
                    // Escape code, followed by one of three kinds of data.
                    if !b.get1() {
                        // The level is the table's level plus the largest
                        // level possible for this run.
                        let entry = match vlc.get(b) {
                            Some(i) if i != escape => &entries[i],
                            _ => invalid!("bad escape code"),
                        };
                        let level = i32::from(entry.level)
                            + largest_level(entries, entry.last, entry.run);
                        (
                            entry.last,
                            i32::from(entry.run),
                            if b.get1() { -level } else { level },
                        )
                    } else if !b.get1() {
                        // The run is the table's run plus the largest run
                        // possible for this level, plus one.
                        let entry = match vlc.get(b) {
                            Some(i) if i != escape => &entries[i],
                            _ => invalid!("bad escape code"),
                        };
                        let level = i32::from(entry.level);
                        let run = i32::from(entry.run)
                            + largest_run(entries, entry.last, entry.level)
                            + 1;
                        (entry.last, run, if b.get1() { -level } else { level })
                    } else {
                        // Fixed length: last, run and level.
                        let last = b.get1();
                        let run = b.get(6) as i32;
                        b.skip(1); // marker bit
                        let mut level = b.get(12) as i32;
                        if level >= 2048 {
                            level -= 4096;
                        }
                        b.skip(1); // marker bit
                        if level == 0 {
                            invalid!("zero level in escape code");
                        }
                        (last, run, level)
                    }
                } else {
                    let entry = &entries[idx];
                    let level = i32::from(entry.level);
                    (
                        entry.last,
                        i32::from(entry.run),
                        if b.get1() { -level } else { level },
                    )
                };
                pos += run + 1;
                if pos > 63 {
                    invalid!("too many coefficients in a block");
                }
                let j = scan[pos as usize] as usize;
                if intra {
                    blk[j] = level as i16;
                } else {
                    // Inter blocks are dequantized right away.
                    blk[j] = dequantize(level, qscale) as i16;
                }
                if last {
                    break;
                }
            }
        }
        if intra {
            if ac_pred {
                // Add the first row or column of the neighboring block.
                let (nx, ny) = if dir == 0 {
                    (bx as i32 - 1, by as i32)
                } else {
                    (bx as i32, by as i32 - 1)
                };
                let sh = if plane == 0 { 1 } else { 0 };
                if nx >= 0 && ny >= 0 && self.mb_avail(nx >> sh, ny >> sh) {
                    let nb = (ny as usize * self.bw[plane] + nx as usize) * 16;
                    let nmb = (ny >> sh) as usize * self.mb_w + (nx >> sh) as usize;
                    let nq = i32::from(self.qtab[nmb]);
                    let same = nmb == mby * self.mb_w + mbx || nq == qscale;
                    for i in 1..8 {
                        let k = if dir == 0 { i } else { 8 + i };
                        let mut p = i32::from(self.acv[plane][nb + k]);
                        if !same {
                            // Rescale for the neighbor's different quantizer.
                            let t = p * nq;
                            let half = qscale >> 1;
                            p = (if t > 0 { t + half } else { t - half }) / qscale;
                        }
                        let j = if dir == 0 { i * 8 } else { i };
                        blk[j] = blk[j].wrapping_add(p as i16);
                    }
                }
            }
            // Remember the first row and column for later blocks.
            for i in 1..8 {
                self.acv[plane][bi * 16 + i] = blk[i * 8];
                self.acv[plane][bi * 16 + 8 + i] = blk[i];
            }
            // Dequantize.
            blk[0] = (i32::from(blk[0]) * dcscale) as i16;
            for v in blk.iter_mut().skip(1) {
                let l = i32::from(*v);
                if l != 0 {
                    *v = dequantize(l, qscale) as i16;
                }
            }
        }
        Ok(())
    }

    // -- macroblocks --

    fn decode_macroblocks(&mut self, b: &mut Bits) -> Result<()> {
        let mb_total = self.mb_w * self.mb_h;
        let mb_bits = (usize::BITS - (mb_total - 1).leading_zeros()).max(1);
        let mut blk = [0i16; 64];
        self.packet_start = 0;
        let mut qscale = self.qscale;
        for mb in 0..mb_total {
            let mbx = mb % self.mb_w;
            let mby = mb / self.mb_w;

            // A video packet header (preceded by stuffing) may start here.
            if mb > 0 && !self.resync_disable {
                let need = 8 - (b.pos & 7) as u32;
                let mut t = *b;
                if t.peek(need) == (1u32 << (need - 1)) - 1 {
                    t.skip(need);
                    let marker_len = 16 + if self.coding_type == 0 { 1 } else { self.fcode };
                    if t.peek(marker_len) == 1 {
                        t.skip(marker_len);
                        let number = t.get(mb_bits) as usize;
                        let q = t.get(5) as i32;
                        if q == 0 {
                            invalid!("zero quantizer in video packet");
                        }
                        if t.get1() {
                            // Header extension: repeats some VOP header data.
                            while t.get1() {}
                            t.skip(1 + self.time_bits + 1 + 2 + 3);
                            if self.coding_type != 0 {
                                t.skip(3);
                            }
                        }
                        if number != mb {
                            invalid!("unexpected macroblock number in video packet");
                        }
                        *b = t;
                        qscale = q;
                        self.packet_start = mb;
                    }
                }
            }

            let skipped = self.coding_type == 1 && b.get1();
            if skipped {
                self.qtab[mb] = qscale as u8;
                self.reset_pred(mbx, mby);
                self.set_mvs(mbx, mby, [(0, 0); 4]);
                self.mc_block(0, mbx * 16, mby * 16, 0, 0, 16);
                self.mc_block(1, mbx * 8, mby * 8, 0, 0, 8);
                self.mc_block(2, mbx * 8, mby * 8, 0, 0, 8);
                continue;
            }

            // Macroblock type and chroma coded block pattern.
            let intra_frame = self.coding_type == 0;
            let idx = loop {
                let code = if intra_frame {
                    self.v_mcbpc_i.get(b)
                } else {
                    self.v_mcbpc_p.get(b)
                };
                match code {
                    None => invalid!("bad macroblock type code"),
                    Some(i) => {
                        let stuffing = if intra_frame { 8 } else { 20 };
                        if i != stuffing {
                            break i;
                        }
                    }
                }
            };
            let cbpc = idx & 3;
            let (intra, dquant, four_mv) = if intra_frame {
                (true, idx >= 4, false)
            } else {
                ((idx & 4) != 0, (idx & 8) != 0, (idx & 16) != 0)
            };
            let ac_pred = intra && b.get1();
            let cbpy = match self.v_cbpy.get(b) {
                Some(v) => v,
                None => invalid!("bad coded block pattern code"),
            };
            let cbpy = if intra { cbpy } else { cbpy ^ 15 };
            let cbp = (cbpy << 2) | cbpc;
            if dquant {
                const DQUANT: [i32; 4] = [-1, -2, 1, 2];
                qscale = (qscale + DQUANT[b.get(2) as usize]).clamp(1, 31);
            }
            self.qtab[mb] = qscale as u8;

            if intra {
                self.set_mvs(mbx, mby, [(0, 0); 4]);
            } else {
                self.reset_pred(mbx, mby);
                self.decode_inter_motion(b, mbx, mby, four_mv)?;
            }

            for n in 0..6 {
                let coded = ((cbp >> (5 - n)) & 1) != 0;
                if !intra && !coded {
                    continue;
                }
                self.decode_block(b, &mut blk, n, intra, coded, ac_pred, mbx, mby, qscale)?;
                let p = if n < 4 { 0 } else { n - 3 };
                let stride = self.stride[p];
                let offset = if p == 0 {
                    (mby * 16 + (n >> 1) * 8) * stride + mbx * 16 + (n & 1) * 8
                } else {
                    mby * 8 * stride + mbx * 8
                };
                idct_store(&blk, &mut self.cur[p], offset, stride, !intra);
            }
        }
        Ok(())
    }

    /// Read the motion vector(s) of an inter macroblock and predict its
    /// pixels (without the residual).
    fn decode_inter_motion(
        &mut self,
        b: &mut Bits,
        mbx: usize,
        mby: usize,
        four_mv: bool,
    ) -> Result<()> {
        if !four_mv {
            let (px, py) = self.pred_mv(mbx, mby, 0);
            let mx = match self.decode_mv_comp(b, px) {
                Some(v) => v,
                None => invalid!("bad motion vector code"),
            };
            let my = match self.decode_mv_comp(b, py) {
                Some(v) => v,
                None => invalid!("bad motion vector code"),
            };
            self.set_mvs(mbx, mby, [(mx, my); 4]);
            self.mc_block(0, mbx * 16, mby * 16, mx, my, 16);
            let cx = chroma_vector_component(mx);
            let cy = chroma_vector_component(my);
            self.mc_block(1, mbx * 8, mby * 8, cx, cy, 8);
            self.mc_block(2, mbx * 8, mby * 8, cx, cy, 8);
        } else {
            let mut mv = [(0i32, 0i32); 4];
            for k in 0..4 {
                let (px, py) = self.pred_mv(mbx, mby, k);
                let mx = match self.decode_mv_comp(b, px) {
                    Some(v) => v,
                    None => invalid!("bad motion vector code"),
                };
                let my = match self.decode_mv_comp(b, py) {
                    Some(v) => v,
                    None => invalid!("bad motion vector code"),
                };
                mv[k] = (mx, my);
                // Later blocks are predicted from this one.
                let bw = self.mb_w * 2;
                let i = ((mby * 2 + (k >> 1)) * bw + mbx * 2 + (k & 1)) * 2;
                self.mvs[i] = mx as i16;
                self.mvs[i + 1] = my as i16;
            }
            for (k, &(mx, my)) in mv.iter().enumerate() {
                let x = mbx * 16 + (k & 1) * 8;
                let y = mby * 16 + (k >> 1) * 8;
                self.mc_block(0, x, y, mx, my, 8);
            }
            let sum_x: i32 = mv.iter().map(|m| m.0).sum();
            let sum_y: i32 = mv.iter().map(|m| m.1).sum();
            let cx = round_chroma(sum_x);
            let cy = round_chroma(sum_y);
            self.mc_block(1, mbx * 8, mby * 8, cx, cy, 8);
            self.mc_block(2, mbx * 8, mby * 8, cx, cy, 8);
        }
        Ok(())
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn chroma_vector_of_a_single_luma_vector() {
        let cases = [
            (0, 0),
            (1, 1),
            (2, 1),
            (3, 1),
            (4, 2),
            (5, 3),
            (-1, -1),
            (-2, -1),
            (-3, -1),
            (-4, -2),
            (-5, -3),
        ];
        for (luma, chroma) in cases {
            assert_eq!(chroma_vector_component(luma), chroma, "luma {}", luma);
        }
    }

    #[test]
    fn chroma_vector_of_four_luma_vectors() {
        let cases = [
            (0, 0),
            (2, 0),
            (3, 1),
            (13, 1),
            (14, 2),
            (15, 2),
            (16, 2),
            (19, 3),
            (-1, 0),
            (-3, -1),
            (-16, -2),
        ];
        for (sum, chroma) in cases {
            assert_eq!(round_chroma(sum), chroma, "sum {}", sum);
        }
    }

    #[test]
    fn dequantization() {
        assert_eq!(dequantize(1, 1), 3);
        assert_eq!(dequantize(1, 2), 5);
        assert_eq!(dequantize(-1, 2), -5);
        assert_eq!(dequantize(-3, 5), -35);
        assert_eq!(dequantize(2000, 31), 2047);
        assert_eq!(dequantize(-2000, 31), -2048);
    }

    #[test]
    fn escape_limits_of_the_run_level_tables() {
        assert_eq!(largest_level(&TCOEF_INTER, false, 0), 12);
        assert_eq!(largest_level(&TCOEF_INTER, true, 0), 3);
        assert_eq!(largest_run(&TCOEF_INTER, false, 1), 26);
        assert_eq!(largest_run(&TCOEF_INTER, true, 1), 40);
        assert_eq!(largest_level(&TCOEF_INTRA, false, 0), 27);
        assert_eq!(largest_level(&TCOEF_INTRA, true, 0), 8);
        assert_eq!(largest_run(&TCOEF_INTRA, false, 1), 14);
        assert_eq!(largest_run(&TCOEF_INTRA, true, 1), 20);
        // Nothing in the tables has this run or level.
        assert_eq!(largest_level(&TCOEF_INTER, false, 63), 0);
        assert_eq!(largest_run(&TCOEF_INTER, false, 63), 0);
    }

    #[test]
    fn run_level_tables_are_consistent() {
        for (entries, first_last) in [(&TCOEF_INTER, 58), (&TCOEF_INTRA, 67)] {
            for (i, e) in entries.iter().enumerate() {
                assert_eq!(e.last, i >= first_last, "entry {}", i);
                assert!(e.level > 0 && e.len > 0 && e.len <= 12);
            }
            // No code is the start of another (including the escape code).
            let mut codes: Vec<(u32, u32)> = entries
                .iter()
                .map(|e| (u32::from(e.code), u32::from(e.len)))
                .collect();
            codes.push((u32::from(TCOEF_ESCAPE.0), u32::from(TCOEF_ESCAPE.1)));
            for (i, &(code_a, len_a)) in codes.iter().enumerate() {
                for (j, &(code_b, len_b)) in codes.iter().enumerate() {
                    if i != j && len_a <= len_b {
                        assert_ne!(code_b >> (len_b - len_a), code_a, "{} and {}", i, j);
                    }
                }
            }
        }
    }

    #[test]
    fn finds_start_codes() {
        let data = [5, 0, 0, 2, 0, 0, 0, 1, 9, 0, 0, 1];
        assert_eq!(find_start_code(&data, 0), Some(5));
        assert_eq!(find_start_code(&data, 6), Some(9));
        assert_eq!(find_start_code(&data, 10), None);
        assert_eq!(find_start_code(&data, 100), None);
        assert_eq!(find_start_code(&[], 0), None);
    }
}
