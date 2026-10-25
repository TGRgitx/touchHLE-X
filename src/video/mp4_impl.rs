/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
// A minimal reader for the video track of an MP4 file: enough to find where
// each frame is and what the decoder configuration data is. It never panics
// on malformed files; anything unexpected makes parsing fail instead.

/// The largest number of samples that will be accepted in a track.
const MAX_SAMPLES: usize = 1 << 20;

/// A frame of the video track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    /// Where the data starts in the file.
    pub offset: u64,
    /// The size of the data in bytes.
    pub size: u32,
    /// When the frame should be shown, in units of the track's timescale.
    pub start: u64,
    /// How long the frame is shown for, in units of the track's timescale.
    pub duration: u32,
}

/// The video track of an MP4 file.
#[derive(Debug, Clone)]
pub struct VideoTrack {
    /// The four character code of the codec, such as mp4v.
    pub codec: [u8; 4],
    /// Time units per second.
    pub timescale: u32,
    pub width: u32,
    pub height: u32,
    /// The codec's decoder configuration data, if there is any. For MPEG-4
    /// video this holds the stream headers.
    pub config: Vec<u8>,
    pub samples: Vec<Sample>,
}

fn be16(d: &[u8], o: usize) -> Option<u32> {
    let b = d.get(o..o.checked_add(2)?)?;
    Some(u32::from(u16::from_be_bytes([b[0], b[1]])))
}

fn be32(d: &[u8], o: usize) -> Option<u32> {
    let b = d.get(o..o.checked_add(4)?)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn be64(d: &[u8], o: usize) -> Option<u64> {
    let b = d.get(o..o.checked_add(8)?)?;
    let mut a = [0u8; 8];
    a.copy_from_slice(b);
    Some(u64::from_be_bytes(a))
}

/// Iterates over the boxes in a byte range. Gives the type of each box and
/// the range of its contents.
struct Boxes<'a> {
    data: &'a [u8],
    pos: usize,
    end: usize,
}

fn boxes(data: &[u8], start: usize, end: usize) -> Boxes<'_> {
    Boxes {
        data,
        pos: start,
        end: end.min(data.len()),
    }
}

impl Iterator for Boxes<'_> {
    type Item = ([u8; 4], usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        let p = self.pos;
        if p.checked_add(8)? > self.end {
            return None;
        }
        let size32 = u64::from(be32(self.data, p)?);
        let kind = [
            self.data[p + 4],
            self.data[p + 5],
            self.data[p + 6],
            self.data[p + 7],
        ];
        let (header, size) = if size32 == 1 {
            if p + 16 > self.end {
                return None;
            }
            (16usize, be64(self.data, p + 8)?)
        } else if size32 == 0 {
            // The box extends to the end of the data.
            (8usize, (self.end - p) as u64)
        } else {
            (8usize, size32)
        };
        if size < header as u64 {
            return None;
        }
        let size = size.min((self.end - p) as u64) as usize;
        self.pos = p + size;
        Some((kind, p + header, p + size))
    }
}

/// Find the first box of a type in a byte range, giving the range of its
/// contents.
fn find(data: &[u8], kind: &[u8; 4], range: (usize, usize)) -> Option<(usize, usize)> {
    boxes(data, range.0, range.1)
        .find(|b| b.0 == *kind)
        .map(|b| (b.1, b.2))
}

/// Read a variable length descriptor size.
fn read_descriptor_len(d: &[u8], mut p: usize) -> Option<(usize, usize)> {
    let mut n = 0usize;
    for _ in 0..4 {
        let b = *d.get(p)?;
        p += 1;
        n = (n << 7) | usize::from(b & 0x7f);
        if (b & 0x80) == 0 {
            break;
        }
    }
    Some((n, p))
}

/// Search the descriptors of an esds box for the decoder specific info.
fn find_decoder_specific_info(d: &[u8], mut p: usize, end: usize, depth: u32) -> Option<&[u8]> {
    if depth > 4 {
        return None;
    }
    while p < end {
        let tag = *d.get(p)?;
        let (len, body) = read_descriptor_len(d, p + 1)?;
        let body_end = body.checked_add(len)?.min(end);
        match tag {
            // DecoderSpecificInfo
            0x05 => return d.get(body..body_end),
            // ES_Descriptor: ES_ID, flags and some optional fields, then
            // more descriptors.
            0x03 => {
                let flags = *d.get(body + 2)?;
                let mut q = body + 3;
                if (flags & 0x80) != 0 {
                    q += 2;
                }
                if (flags & 0x40) != 0 {
                    q += 1 + usize::from(*d.get(q)?);
                }
                if (flags & 0x20) != 0 {
                    q += 2;
                }
                if let Some(info) = find_decoder_specific_info(d, q, body_end, depth + 1) {
                    return Some(info);
                }
            }
            // DecoderConfigDescriptor: 13 bytes of fields, then descriptors.
            0x04 => {
                if let Some(info) = find_decoder_specific_info(d, body + 13, body_end, depth + 1) {
                    return Some(info);
                }
            }
            _ => {}
        }
        p = body_end;
    }
    None
}

impl VideoTrack {
    /// Find the first video track in the contents of an MP4 file.
    pub fn parse(file: &[u8]) -> Result<VideoTrack, &'static str> {
        let moov = find(file, b"moov", (0, file.len())).ok_or("no moov box")?;
        for (kind, start, end) in boxes(file, moov.0, moov.1) {
            if kind != *b"trak" {
                continue;
            }
            if let Some(track) = parse_track(file, (start, end)) {
                return Ok(track);
            }
        }
        Err("no video track")
    }

    /// The contents of a sample, if it is inside the file.
    pub fn sample_data<'a>(&self, file: &'a [u8], index: usize) -> Option<&'a [u8]> {
        let sample = self.samples.get(index)?;
        let start = usize::try_from(sample.offset).ok()?;
        let end = start.checked_add(sample.size as usize)?;
        file.get(start..end)
    }

    /// The total duration in seconds.
    pub fn duration_seconds(&self) -> f64 {
        let ticks: u64 = self.samples.iter().map(|s| u64::from(s.duration)).sum();
        ticks as f64 / f64::from(self.timescale.max(1))
    }
}

/// Parse a trak box, if it's a video track.
fn parse_track(file: &[u8], trak: (usize, usize)) -> Option<VideoTrack> {
    let mdia = find(file, b"mdia", trak)?;
    let hdlr = find(file, b"hdlr", mdia)?;
    if file.get(hdlr.0 + 8..hdlr.0 + 12)? != b"vide" {
        return None;
    }
    let mdhd = find(file, b"mdhd", mdia)?;
    let version = *file.get(mdhd.0)?;
    let timescale = be32(file, mdhd.0 + if version == 0 { 12 } else { 20 })?;
    let minf = find(file, b"minf", mdia)?;
    let stbl = find(file, b"stbl", minf)?;

    // The sample description: codec, size and configuration.
    let stsd = find(file, b"stsd", stbl)?;
    let (codec, entry_start, entry_end) = boxes(file, stsd.0 + 8, stsd.1).next()?;
    let width = be16(file, entry_start + 24)?;
    let height = be16(file, entry_start + 26)?;
    let config = match find(file, b"esds", (entry_start + 78, entry_end)) {
        Some(esds) => {
            find_decoder_specific_info(file, esds.0 + 4, esds.1, 0).unwrap_or(&[])
        }
        None => &[],
    };

    // Sample sizes.
    let stsz = find(file, b"stsz", stbl)?;
    let uniform_size = be32(file, stsz.0 + 4)?;
    let count = be32(file, stsz.0 + 8)? as usize;
    if count > MAX_SAMPLES {
        return None;
    }
    let mut sizes = Vec::with_capacity(count);
    for i in 0..count {
        sizes.push(if uniform_size != 0 {
            uniform_size
        } else {
            be32(file, stsz.0 + 12 + 4 * i)?
        });
    }

    // Sample durations.
    let stts = find(file, b"stts", stbl)?;
    let stts_count = be32(file, stts.0 + 4)? as usize;
    let mut durations: Vec<u32> = Vec::new();
    for i in 0..stts_count {
        if durations.len() >= count {
            break;
        }
        let run = be32(file, stts.0 + 8 + 8 * i)? as usize;
        let delta = be32(file, stts.0 + 12 + 8 * i)?;
        let room = count - durations.len();
        durations.resize(durations.len() + run.min(room), delta);
    }

    // Which samples are in which chunk.
    let stsc = find(file, b"stsc", stbl)?;
    let stsc_count = be32(file, stsc.0 + 4)? as usize;
    let mut chunk_runs = Vec::new();
    for i in 0..stsc_count.min(MAX_SAMPLES) {
        let first_chunk = be32(file, stsc.0 + 8 + 12 * i)?;
        let samples_per_chunk = be32(file, stsc.0 + 12 + 12 * i)?;
        chunk_runs.push((first_chunk, samples_per_chunk));
    }

    // Where each chunk is.
    let mut chunk_offsets: Vec<u64> = Vec::new();
    if let Some(stco) = find(file, b"stco", stbl) {
        let n = (be32(file, stco.0 + 4)? as usize).min(MAX_SAMPLES);
        for i in 0..n {
            chunk_offsets.push(u64::from(be32(file, stco.0 + 8 + 4 * i)?));
        }
    } else {
        let co64 = find(file, b"co64", stbl)?;
        let n = (be32(file, co64.0 + 4)? as usize).min(MAX_SAMPLES);
        for i in 0..n {
            chunk_offsets.push(be64(file, co64.0 + 8 + 8 * i)?);
        }
    }

    let mut samples = Vec::with_capacity(count);
    let mut run_index = 0;
    let mut time = 0u64;
    for (chunk_index, &chunk_offset) in chunk_offsets.iter().enumerate() {
        let chunk_number = chunk_index as u64 + 1;
        while run_index + 1 < chunk_runs.len() && u64::from(chunk_runs[run_index + 1].0) <= chunk_number {
            run_index += 1;
        }
        let per_chunk = match chunk_runs.get(run_index) {
            Some(&(first_chunk, n)) if u64::from(first_chunk) <= chunk_number => n,
            _ => 0,
        };
        let mut offset = chunk_offset;
        for _ in 0..per_chunk {
            let index = samples.len();
            if index >= count {
                break;
            }
            let duration = durations.get(index).copied().unwrap_or(0);
            samples.push(Sample {
                offset,
                size: sizes[index],
                start: time,
                duration,
            });
            offset = offset.saturating_add(u64::from(sizes[index]));
            time = time.saturating_add(u64::from(duration));
        }
    }

    Some(VideoTrack {
        codec,
        timescale,
        width,
        height,
        config: config.to_vec(),
        samples,
    })
}
