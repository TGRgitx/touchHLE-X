/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Video decoding, for playing the movies that apps ask for.
//!
//! The code of the larger modules is pulled in with `include!` so that it
//! is left alone by rustfmt: it contains big tables and arithmetic that is
//! written to line up with the specification it implements.

#![allow(dead_code)]
#![allow(clippy::too_many_arguments, clippy::needless_range_loop)]
#![allow(clippy::manual_range_contains, clippy::identity_op)]

/// Finding the frames of the video track in an MP4 file.
pub mod mp4 {
    include!("video/mp4_impl.rs");
}

/// A decoder for MPEG-4 Part 2 (Simple Profile) video.
pub mod mpeg4 {
    include!("video/mpeg4_impl.rs");
}

mod mpeg4_tables {
    include!("video/mpeg4_tables_impl.rs");
}

#[cfg(test)]
mod tests;
