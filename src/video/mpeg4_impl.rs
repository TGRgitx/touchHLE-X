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
// The decoder has been tested for bit-exactness against libavcodec's output
// (with its C "simple" IDCT, which is what the IDCT here is equivalent to).
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

    fn from16(bits: u32, codes: &[(u16, u8)]) -> Vlc {
        let codes: Vec<(u32, u32)> = codes
            .iter()
            .map(|&(c, l)| (u32::from(c), u32::from(l)))
            .collect();
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
    let mut i = from;
    while i + 3 <= data.len() {
        if data[i + 2] > 1 {
            i += 3;
        } else if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            return Some(i);
        } else {
            i += 1;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Inverse DCT: same arithmetic as the "simple" IDCT in libavcodec, but using
// 64-bit integers so that garbage input can't cause overflow panics.
// ---------------------------------------------------------------------------

const W1: i64 = 22725;
const W2: i64 = 21407;
const W3: i64 = 19266;
const W4: i64 = 16383;
const W5: i64 = 12873;
const W6: i64 = 8867;
const W7: i64 = 4520;
const ROW_SHIFT: u32 = 11;
const COL_SHIFT: u32 = 20;

fn idct_row(r: &mut [i16]) {
    if r[1..8].iter().all(|&v| v == 0) {
        let t = (i32::from(r[0]) * 8) as i16;
        r[..8].fill(t);
        return;
    }
    let mut v = [0i64; 8];
    for (dst, &src) in v.iter_mut().zip(r.iter()) {
        *dst = i64::from(src);
    }
    let mut a0 = W4 * v[0] + (1i64 << (ROW_SHIFT - 1));
    let mut a1 = a0;
    let mut a2 = a0;
    let mut a3 = a0;
    a0 += W2 * v[2];
    a1 += W6 * v[2];
    a2 -= W6 * v[2];
    a3 -= W2 * v[2];
    let mut b0 = W1 * v[1] + W3 * v[3];
    let mut b1 = W3 * v[1] - W7 * v[3];
    let mut b2 = W5 * v[1] - W1 * v[3];
    let mut b3 = W7 * v[1] - W5 * v[3];
    a0 += W4 * v[4] + W6 * v[6];
    a1 += -W4 * v[4] - W2 * v[6];
    a2 += -W4 * v[4] + W2 * v[6];
    a3 += W4 * v[4] - W6 * v[6];
    b0 += W5 * v[5] + W7 * v[7];
    b1 += -W1 * v[5] - W5 * v[7];
    b2 += W7 * v[5] + W3 * v[7];
    b3 += W3 * v[5] - W1 * v[7];
    r[0] = ((a0 + b0) >> ROW_SHIFT) as i16;
    r[7] = ((a0 - b0) >> ROW_SHIFT) as i16;
    r[1] = ((a1 + b1) >> ROW_SHIFT) as i16;
    r[6] = ((a1 - b1) >> ROW_SHIFT) as i16;
    r[2] = ((a2 + b2) >> ROW_SHIFT) as i16;
    r[5] = ((a2 - b2) >> ROW_SHIFT) as i16;
    r[3] = ((a3 + b3) >> ROW_SHIFT) as i16;
    r[4] = ((a3 - b3) >> ROW_SHIFT) as i16;
}

/// The column pass for column x of a block. Gives the 8 results, which still
/// have to be clipped.
fn idct_col(blk: &[i16; 64], x: usize) -> [i64; 8] {
    let c = |row: usize| i64::from(blk[row * 8 + x]);
    let mut a0 = W4 * (c(0) + ((1i64 << (COL_SHIFT - 1)) / W4));
    let mut a1 = a0;
    let mut a2 = a0;
    let mut a3 = a0;
    a0 += W2 * c(2);
    a1 += W6 * c(2);
    a2 -= W6 * c(2);
    a3 -= W2 * c(2);
    let mut b0 = W1 * c(1) + W3 * c(3);
    let mut b1 = W3 * c(1) - W7 * c(3);
    let mut b2 = W5 * c(1) - W1 * c(3);
    let mut b3 = W7 * c(1) - W5 * c(3);
    a0 += W4 * c(4);
    a1 -= W4 * c(4);
    a2 -= W4 * c(4);
    a3 += W4 * c(4);
    b0 += W5 * c(5);
    b1 -= W1 * c(5);
    b2 += W7 * c(5);
    b3 += W3 * c(5);
    a0 += W6 * c(6);
    a1 -= W2 * c(6);
    a2 += W2 * c(6);
    a3 -= W6 * c(6);
    b0 += W7 * c(7);
    b1 -= W5 * c(7);
    b2 += W3 * c(7);
    b3 -= W1 * c(7);
    [
        (a0 + b0) >> COL_SHIFT,
        (a1 + b1) >> COL_SHIFT,
        (a2 + b2) >> COL_SHIFT,
        (a3 + b3) >> COL_SHIFT,
        (a3 - b3) >> COL_SHIFT,
        (a2 - b2) >> COL_SHIFT,
        (a1 - b1) >> COL_SHIFT,
        (a0 - b0) >> COL_SHIFT,
    ]
}

/// Do the IDCT of a block and either store the result to or add it to the
/// 8x8 area starting at `offset` in `dst`.
fn idct_store(blk: &mut [i16; 64], dst: &mut [u8], offset: usize, stride: usize, add: bool) {
    for row in 0..8 {
        idct_row(&mut blk[row * 8..row * 8 + 8]);
    }
    for x in 0..8 {
        let out = idct_col(blk, x);
        for (y, &o) in out.iter().enumerate() {
            let i = offset + y * stride + x;
            let mut v = o;
            if add {
                v += i64::from(dst[i]);
            }
            dst[i] = v.clamp(0, 255) as u8;
        }
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

fn median3(a: i32, b: i32, c: i32) -> i32 {
    let mx = a.max(b);
    let mn = a.min(b);
    c.min(mx).max(mn)
}

/// How the four luma vectors of a macroblock are combined into the chroma
/// vector (in half pixel units), as specified for H.263 style prediction.
fn round_chroma(x: i32) -> i32 {
    const ROUND: [i32; 16] = [0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2];
    ROUND[(x & 15) as usize] + ((x >> 3) & !1)
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
    /// Convert to RGBA (8 bits per channel, BT.601 limited range) in `out`,
    /// which must have room for width x height x 4 bytes.
    pub fn write_rgba(&self, out: &mut [u8]) {
        for row in 0..self.height {
            for col in 0..self.width {
                let y = i32::from(self.y[row * self.y_stride + col]) - 16;
                let u_i = (row / 2) * self.uv_stride + col / 2;
                let u = i32::from(self.u[u_i]) - 128;
                let v = i32::from(self.v[u_i]) - 128;
                let c = 298 * y;
                let r = (c + 409 * v + 128) >> 8;
                let g = (c - 100 * u - 208 * v + 128) >> 8;
                let b = (c + 516 * u + 128) >> 8;
                let o = (row * self.width + col) * 4;
                if let Some(px) = out.get_mut(o..o + 4) {
                    px[0] = r.clamp(0, 255) as u8;
                    px[1] = g.clamp(0, 255) as u8;
                    px[2] = b.clamp(0, 255) as u8;
                    px[3] = 255;
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
    // Indexed by intra flag, last flag, then run or level respectively.
    max_level: [[[i32; 64]; 2]; 2],
    max_run: [[[i32; 64]; 2]; 2],
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    pub fn new() -> Decoder {
        let mut max_level = [[[0i32; 64]; 2]; 2];
        let mut max_run = [[[0i32; 64]; 2]; 2];
        for intra in 0..2 {
            let (level, run, last_start) = if intra == 1 {
                (&TCOEF_INTRA_LEVEL, &TCOEF_INTRA_RUN, TCOEF_INTRA_LAST_START)
            } else {
                (&TCOEF_INTER_LEVEL, &TCOEF_INTER_RUN, TCOEF_INTER_LAST_START)
            };
            for i in 0..102 {
                let last = usize::from(i >= last_start);
                let (l, r) = (i32::from(level[i]), i32::from(run[i]));
                let ml = &mut max_level[intra][last][r as usize];
                *ml = (*ml).max(l);
                let mr = &mut max_run[intra][last][l as usize];
                *mr = (*mr).max(r);
            }
        }
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
            v_inter: Vlc::from16(12, &TCOEF_INTER_VLC),
            v_intra: Vlc::from16(12, &TCOEF_INTRA_VLC),
            v_mv: Vlc::from8(12, &MV_VLC),
            v_dcl: Vlc::from8(12, &DC_LUM_VLC),
            v_dcc: Vlc::from8(12, &DC_CHROM_VLC),
            v_cbpy: Vlc::from8(6, &CBPY_VLC),
            v_mcbpc_i: Vlc::from8(9, &MCBPC_INTRA_VLC),
            v_mcbpc_p: Vlc::from8(9, &MCBPC_INTER_VLC),
            max_level,
            max_run,
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
    fn pred_mv(&self, mbx: usize, mby: usize, n: usize) -> (i32, i32) {
        const OFFC: [i32; 4] = [2, 1, 1, -1];
        let bw = (self.mb_w * 2) as i32;
        let bx = (mbx * 2 + (n & 1)) as i32;
        let by = (mby * 2 + (n >> 1)) as i32;
        let pos = [(bx - 1, by), (bx, by - 1), (bx + OFFC[n], by - 1)];
        let mut vals = [(0i32, 0i32); 3];
        let mut count = 0;
        let mut last = 0;
        for (k, &(x, y)) in pos.iter().enumerate() {
            let avail = x >= 0
                && y >= 0
                && x < bw
                && y < (self.mb_h * 2) as i32
                && self.mb_avail(x >> 1, y >> 1);
            if avail {
                let i = (y as usize * bw as usize + x as usize) * 2;
                vals[k] = (i32::from(self.mvs[i]), i32::from(self.mvs[i + 1]));
                count += 1;
                last = k;
            }
        }
        if count == 1 {
            return vals[last];
        }
        (
            median3(vals[0].0, vals[1].0, vals[2].0),
            median3(vals[0].1, vals[1].1, vals[2].1),
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
            let (vlc, levels, runs, last_start) = if intra {
                (
                    &self.v_intra,
                    &TCOEF_INTRA_LEVEL,
                    &TCOEF_INTRA_RUN,
                    TCOEF_INTRA_LAST_START,
                )
            } else {
                (
                    &self.v_inter,
                    &TCOEF_INTER_LEVEL,
                    &TCOEF_INTER_RUN,
                    TCOEF_INTER_LAST_START,
                )
            };
            let ii = usize::from(intra);
            let qmul = qscale * 2;
            let qadd = (qscale - 1) | 1;
            loop {
                let idx = match vlc.get(b) {
                    Some(i) => i,
                    None => invalid!("bad coefficient code"),
                };
                let (last, run, level) = if idx == 102 {
                    // Escape code, followed by one of three kinds of data.
                    if !b.get1() {
                        // The level is the table's level plus the largest
                        // level possible for this run.
                        let i = match vlc.get(b) {
                            Some(i) if i != 102 => i,
                            _ => invalid!("bad escape code"),
                        };
                        let last = i >= last_start;
                        let run = i32::from(runs[i]);
                        let level = i32::from(levels[i])
                            + self.max_level[ii][usize::from(last)][run as usize];
                        (last, run, if b.get1() { -level } else { level })
                    } else if !b.get1() {
                        // The run is the table's run plus the largest run
                        // possible for this level, plus one.
                        let i = match vlc.get(b) {
                            Some(i) if i != 102 => i,
                            _ => invalid!("bad escape code"),
                        };
                        let last = i >= last_start;
                        let level = i32::from(levels[i]);
                        let run = i32::from(runs[i])
                            + self.max_run[ii][usize::from(last)][level as usize]
                            + 1;
                        (last, run, if b.get1() { -level } else { level })
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
                    let level = i32::from(levels[idx]);
                    (
                        idx >= last_start,
                        i32::from(runs[idx]),
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
                    let v = if level > 0 {
                        level * qmul + qadd
                    } else {
                        level * qmul - qadd
                    };
                    blk[j] = v.clamp(-2048, 2047) as i16;
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
            let qmul = qscale * 2;
            let qadd = (qscale - 1) | 1;
            blk[0] = (i32::from(blk[0]) * dcscale) as i16;
            for v in blk.iter_mut().skip(1) {
                let l = i32::from(*v);
                if l != 0 {
                    let x = if l > 0 { l * qmul + qadd } else { l * qmul - qadd };
                    *v = x.clamp(-2048, 2047) as i16;
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
                idct_store(&mut blk, &mut self.cur[p], offset, stride, !intra);
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
            let cx = (mx >> 1) | (mx & 1);
            let cy = (my >> 1) | (my & 1);
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
