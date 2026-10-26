/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
// Constant tables for the MPEG-4 Part 2 decoder.
//
// The variable length code tables are the ones from Annex B of
// ISO/IEC 14496-2. Each code is stored with its length in bits; the run/level
// tables also say what each code stands for.

/// One entry of a run/level code table: the code (value and length in bits),
/// whether it ends the block (`last`), and the run of zero coefficients
/// before the coefficient and the absolute value (`level`) of the coefficient.
/// The sign of the coefficient follows the code as a separate bit.
#[derive(Clone, Copy)]
pub struct Tcoef {
    pub code: u16,
    pub len: u8,
    pub last: bool,
    pub run: u8,
    pub level: u8,
}

const fn tc(code: u16, len: u8, last: bool, run: u8, level: u8) -> Tcoef {
    Tcoef {
        code,
        len,
        last,
        run,
        level,
    }
}

/// The code that introduces an escape sequence in both tables.
pub const TCOEF_ESCAPE: (u16, u8) = (3, 7);

/// Run/level codes for inter blocks, in the order of the standard's table.
pub const TCOEF_INTER: [Tcoef; 102] = [
    tc(2, 2, false, 0, 1), tc(15, 4, false, 0, 2),
    tc(21, 6, false, 0, 3), tc(23, 7, false, 0, 4),
    tc(31, 8, false, 0, 5), tc(37, 9, false, 0, 6),
    tc(36, 9, false, 0, 7), tc(33, 10, false, 0, 8),
    tc(32, 10, false, 0, 9), tc(7, 11, false, 0, 10),
    tc(6, 11, false, 0, 11), tc(32, 11, false, 0, 12),
    tc(6, 3, false, 1, 1), tc(20, 6, false, 1, 2),
    tc(30, 8, false, 1, 3), tc(15, 10, false, 1, 4),
    tc(33, 11, false, 1, 5), tc(80, 12, false, 1, 6),
    tc(14, 4, false, 2, 1), tc(29, 8, false, 2, 2),
    tc(14, 10, false, 2, 3), tc(81, 12, false, 2, 4),
    tc(13, 5, false, 3, 1), tc(35, 9, false, 3, 2),
    tc(13, 10, false, 3, 3), tc(12, 5, false, 4, 1),
    tc(34, 9, false, 4, 2), tc(82, 12, false, 4, 3),
    tc(11, 5, false, 5, 1), tc(12, 10, false, 5, 2),
    tc(83, 12, false, 5, 3), tc(19, 6, false, 6, 1),
    tc(11, 10, false, 6, 2), tc(84, 12, false, 6, 3),
    tc(18, 6, false, 7, 1), tc(10, 10, false, 7, 2),
    tc(17, 6, false, 8, 1), tc(9, 10, false, 8, 2),
    tc(16, 6, false, 9, 1), tc(8, 10, false, 9, 2),
    tc(22, 7, false, 10, 1), tc(85, 12, false, 10, 2),
    tc(21, 7, false, 11, 1), tc(20, 7, false, 12, 1),
    tc(28, 8, false, 13, 1), tc(27, 8, false, 14, 1),
    tc(33, 9, false, 15, 1), tc(32, 9, false, 16, 1),
    tc(31, 9, false, 17, 1), tc(30, 9, false, 18, 1),
    tc(29, 9, false, 19, 1), tc(28, 9, false, 20, 1),
    tc(27, 9, false, 21, 1), tc(26, 9, false, 22, 1),
    tc(34, 11, false, 23, 1), tc(35, 11, false, 24, 1),
    tc(86, 12, false, 25, 1), tc(87, 12, false, 26, 1),
    tc(7, 4, true, 0, 1), tc(25, 9, true, 0, 2),
    tc(5, 11, true, 0, 3), tc(15, 6, true, 1, 1),
    tc(4, 11, true, 1, 2), tc(14, 6, true, 2, 1),
    tc(13, 6, true, 3, 1), tc(12, 6, true, 4, 1),
    tc(19, 7, true, 5, 1), tc(18, 7, true, 6, 1),
    tc(17, 7, true, 7, 1), tc(16, 7, true, 8, 1),
    tc(26, 8, true, 9, 1), tc(25, 8, true, 10, 1),
    tc(24, 8, true, 11, 1), tc(23, 8, true, 12, 1),
    tc(22, 8, true, 13, 1), tc(21, 8, true, 14, 1),
    tc(20, 8, true, 15, 1), tc(19, 8, true, 16, 1),
    tc(24, 9, true, 17, 1), tc(23, 9, true, 18, 1),
    tc(22, 9, true, 19, 1), tc(21, 9, true, 20, 1),
    tc(20, 9, true, 21, 1), tc(19, 9, true, 22, 1),
    tc(18, 9, true, 23, 1), tc(17, 9, true, 24, 1),
    tc(7, 10, true, 25, 1), tc(6, 10, true, 26, 1),
    tc(5, 10, true, 27, 1), tc(4, 10, true, 28, 1),
    tc(36, 11, true, 29, 1), tc(37, 11, true, 30, 1),
    tc(38, 11, true, 31, 1), tc(39, 11, true, 32, 1),
    tc(88, 12, true, 33, 1), tc(89, 12, true, 34, 1),
    tc(90, 12, true, 35, 1), tc(91, 12, true, 36, 1),
    tc(92, 12, true, 37, 1), tc(93, 12, true, 38, 1),
    tc(94, 12, true, 39, 1), tc(95, 12, true, 40, 1),
];

/// Run/level codes for intra blocks, in the order of the standard's table.
pub const TCOEF_INTRA: [Tcoef; 102] = [
    tc(2, 2, false, 0, 1), tc(6, 3, false, 0, 2),
    tc(15, 4, false, 0, 3), tc(13, 5, false, 0, 4),
    tc(12, 5, false, 0, 5), tc(21, 6, false, 0, 6),
    tc(19, 6, false, 0, 7), tc(18, 6, false, 0, 8),
    tc(23, 7, false, 0, 9), tc(31, 8, false, 0, 10),
    tc(30, 8, false, 0, 11), tc(29, 8, false, 0, 12),
    tc(37, 9, false, 0, 13), tc(36, 9, false, 0, 14),
    tc(35, 9, false, 0, 15), tc(33, 9, false, 0, 16),
    tc(33, 10, false, 0, 17), tc(32, 10, false, 0, 18),
    tc(15, 10, false, 0, 19), tc(14, 10, false, 0, 20),
    tc(7, 11, false, 0, 21), tc(6, 11, false, 0, 22),
    tc(32, 11, false, 0, 23), tc(33, 11, false, 0, 24),
    tc(80, 12, false, 0, 25), tc(81, 12, false, 0, 26),
    tc(82, 12, false, 0, 27), tc(14, 4, false, 1, 1),
    tc(20, 6, false, 1, 2), tc(22, 7, false, 1, 3),
    tc(28, 8, false, 1, 4), tc(32, 9, false, 1, 5),
    tc(31, 9, false, 1, 6), tc(13, 10, false, 1, 7),
    tc(34, 11, false, 1, 8), tc(83, 12, false, 1, 9),
    tc(85, 12, false, 1, 10), tc(11, 5, false, 2, 1),
    tc(21, 7, false, 2, 2), tc(30, 9, false, 2, 3),
    tc(12, 10, false, 2, 4), tc(86, 12, false, 2, 5),
    tc(17, 6, false, 3, 1), tc(27, 8, false, 3, 2),
    tc(29, 9, false, 3, 3), tc(11, 10, false, 3, 4),
    tc(16, 6, false, 4, 1), tc(34, 9, false, 4, 2),
    tc(10, 10, false, 4, 3), tc(13, 6, false, 5, 1),
    tc(28, 9, false, 5, 2), tc(8, 10, false, 5, 3),
    tc(18, 7, false, 6, 1), tc(27, 9, false, 6, 2),
    tc(84, 12, false, 6, 3), tc(20, 7, false, 7, 1),
    tc(26, 9, false, 7, 2), tc(87, 12, false, 7, 3),
    tc(25, 8, false, 8, 1), tc(9, 10, false, 8, 2),
    tc(24, 8, false, 9, 1), tc(35, 11, false, 9, 2),
    tc(23, 8, false, 10, 1), tc(25, 9, false, 11, 1),
    tc(24, 9, false, 12, 1), tc(7, 10, false, 13, 1),
    tc(88, 12, false, 14, 1), tc(7, 4, true, 0, 1),
    tc(12, 6, true, 0, 2), tc(22, 8, true, 0, 3),
    tc(23, 9, true, 0, 4), tc(6, 10, true, 0, 5),
    tc(5, 11, true, 0, 6), tc(4, 11, true, 0, 7),
    tc(89, 12, true, 0, 8), tc(15, 6, true, 1, 1),
    tc(22, 9, true, 1, 2), tc(5, 10, true, 1, 3),
    tc(14, 6, true, 2, 1), tc(4, 10, true, 2, 2),
    tc(17, 7, true, 3, 1), tc(36, 11, true, 3, 2),
    tc(16, 7, true, 4, 1), tc(37, 11, true, 4, 2),
    tc(19, 7, true, 5, 1), tc(90, 12, true, 5, 2),
    tc(21, 8, true, 6, 1), tc(91, 12, true, 6, 2),
    tc(20, 8, true, 7, 1), tc(19, 8, true, 8, 1),
    tc(26, 8, true, 9, 1), tc(21, 9, true, 10, 1),
    tc(20, 9, true, 11, 1), tc(19, 9, true, 12, 1),
    tc(18, 9, true, 13, 1), tc(17, 9, true, 14, 1),
    tc(38, 11, true, 15, 1), tc(39, 11, true, 16, 1),
    tc(92, 12, true, 17, 1), tc(93, 12, true, 18, 1),
    tc(94, 12, true, 19, 1), tc(95, 12, true, 20, 1),
];

/// Motion vector difference magnitude codes.
pub const MV_VLC: [(u8, u8); 33] = [
    (1, 1), (1, 2), (1, 3), (1, 4), (3, 6), (5, 7), (4, 7), (3, 7),
    (11, 9), (10, 9), (9, 9), (17, 10), (16, 10), (15, 10), (14, 10), (13, 10),
    (12, 10), (11, 10), (10, 10), (9, 10), (8, 10), (7, 10), (6, 10), (5, 10),
    (4, 10), (7, 11), (6, 11), (5, 11), (4, 11), (3, 11), (2, 11), (3, 12),
    (2, 12),
];

/// Intra DC size codes for luma.
pub const DC_LUM_VLC: [(u8, u8); 13] = [
    (3, 3), (3, 2), (2, 2), (2, 3), (1, 3), (1, 4), (1, 5), (1, 6),
    (1, 7), (1, 8), (1, 9), (1, 10), (1, 11),
];

/// Intra DC size codes for chroma.
pub const DC_CHROM_VLC: [(u8, u8); 13] = [
    (3, 2), (2, 2), (1, 2), (1, 3), (1, 4), (1, 5), (1, 6), (1, 7),
    (1, 8), (1, 9), (1, 10), (1, 11), (1, 12),
];

/// Coded block pattern codes for luma.
pub const CBPY_VLC: [(u8, u8); 16] = [
    (3, 4), (5, 5), (4, 5), (9, 4), (3, 5), (7, 4), (2, 6), (11, 4),
    (2, 5), (3, 6), (5, 4), (10, 4), (4, 4), (8, 4), (6, 4), (3, 2),
];

/// Macroblock type and chroma pattern codes in I frames.
pub const MCBPC_INTRA_VLC: [(u8, u8); 9] = [
    (1, 1), (1, 3), (2, 3), (3, 3), (1, 4), (1, 6), (2, 6), (3, 6),
    (1, 9),
];

/// Macroblock type and chroma pattern codes in P frames.
pub const MCBPC_INTER_VLC: [(u8, u8); 21] = [
    (1, 1), (3, 4), (2, 4), (5, 6), (3, 5), (4, 8), (3, 8), (3, 7),
    (3, 3), (7, 7), (6, 7), (5, 9), (4, 6), (4, 9), (3, 9), (2, 9),
    (2, 3), (5, 7), (4, 7), (5, 8), (1, 9),
];

/// The zigzag scan order.
pub const ZIGZAG_SCAN: [u8; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5,
    12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// The alternate horizontal scan order.
pub const ALT_HORIZONTAL_SCAN: [u8; 64] = [
    0, 1, 2, 3, 8, 9, 16, 17, 10, 11, 4, 5, 6, 7, 15, 14,
    13, 12, 19, 18, 24, 25, 32, 33, 26, 27, 20, 21, 22, 23, 28, 29,
    30, 31, 34, 35, 40, 41, 48, 49, 42, 43, 36, 37, 38, 39, 44, 45,
    46, 47, 50, 51, 56, 57, 58, 59, 52, 53, 54, 55, 60, 61, 62, 63,
];

/// The alternate vertical scan order.
pub const ALT_VERTICAL_SCAN: [u8; 64] = [
    0, 8, 16, 24, 1, 9, 2, 10, 17, 25, 32, 40, 48, 56, 57, 49,
    41, 33, 26, 18, 3, 11, 4, 12, 19, 27, 34, 42, 50, 58, 35, 43,
    51, 59, 20, 28, 5, 13, 6, 14, 21, 29, 36, 44, 52, 60, 37, 45,
    53, 61, 22, 30, 7, 15, 23, 31, 38, 46, 54, 62, 39, 47, 55, 63,
];
