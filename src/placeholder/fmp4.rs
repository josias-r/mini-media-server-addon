use super::{
    AUDIO_SEG_DURATION, AUDIO_TRACK_ID, PlaceholderState, SEGMENTS_PER_LOOP, VIDEO_SEG_DURATION,
    VIDEO_TIMESCALE, VIDEO_TRACK_ID,
};
use bytes::Bytes;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
struct SegmentTemplate {
    raw: Vec<u8>,
    mfhd_seq_offset: usize,
    video_tfdt_offset: usize,
    video_tfdt_v1: bool,
    audio_tfdt_offset: usize,
    audio_tfdt_v1: bool,
}

#[derive(Debug)]
pub struct PlaceholderBank {
    pub header: Bytes,
    pub slot_size: u64,
    pub total_fragments: u64,
    pub total_len: u64,
    states: HashMap<PlaceholderState, [SegmentTemplate; SEGMENTS_PER_LOOP]>,
}

fn parse_boxes(
    data: &[u8],
    start: usize,
    end: usize,
) -> Result<Vec<([u8; 4], usize, usize)>, String> {
    if end > data.len() || start > end {
        return Err("MP4 slice bounds out of range".to_string());
    }
    let mut pos = start;
    let mut out = Vec::new();
    while pos + 8 <= end {
        let raw_sz = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        let typ: [u8; 4] = data[pos + 4..pos + 8].try_into().unwrap();
        let sz = if raw_sz == 0 { end - pos } else { raw_sz };
        if sz < 8 || pos + sz > end {
            return Err(format!(
                "Invalid MP4 box {:?} size {} at {}",
                String::from_utf8_lossy(&typ),
                sz,
                pos
            ));
        }
        out.push((typ, pos, sz));
        pos += sz;
    }
    if pos != end {
        return Err(format!("Trailing {} unparsed MP4 bytes", end - pos));
    }
    Ok(out)
}

fn parse_segment_template(raw: Vec<u8>) -> Result<SegmentTemplate, String> {
    let top = parse_boxes(&raw, 0, raw.len())?;
    if top.len() != 2 || &top[0].0 != b"moof" || &top[1].0 != b"mdat" {
        return Err("Segment must consist of [moof, mdat]".to_string());
    }
    let moof_end = top[0].2;
    let mut mfhd_seq_offset = None;
    let mut video_tfdt = None;
    let mut audio_tfdt = None;

    for (ctyp, cpos, csz) in parse_boxes(&raw, 8, moof_end)? {
        if &ctyp == b"mfhd" {
            if csz < 16 {
                return Err("mfhd box too small".to_string());
            }
            mfhd_seq_offset = Some(cpos + 12);
        } else if &ctyp == b"traf" {
            let mut track_id = None;
            let mut tfdt_info = None;
            for (ttyp, tpos, tsz) in parse_boxes(&raw, cpos + 8, cpos + csz)? {
                if &ttyp == b"tfhd" {
                    if tsz < 16 {
                        return Err("tfhd box too small".to_string());
                    }
                    track_id = Some(u32::from_be_bytes(
                        raw[tpos + 12..tpos + 16].try_into().unwrap(),
                    ));
                } else if &ttyp == b"tfdt" {
                    if tsz < 16 {
                        return Err("tfdt box too small".to_string());
                    }
                    let ver = raw[tpos + 8];
                    let is_v1 = match ver {
                        0 => false,
                        1 => {
                            if tsz < 20 {
                                return Err("tfdt v1 box too small".to_string());
                            }
                            true
                        }
                        _ => return Err(format!("Unsupported tfdt version {}", ver)),
                    };
                    tfdt_info = Some((tpos + 12, is_v1));
                }
            }
            match (track_id, tfdt_info) {
                (Some(VIDEO_TRACK_ID), Some(info)) => video_tfdt = Some(info),
                (Some(AUDIO_TRACK_ID), Some(info)) => audio_tfdt = Some(info),
                _ => {}
            }
        }
    }

    let mfhd_seq_offset = mfhd_seq_offset.ok_or_else(|| "Missing mfhd in segment".to_string())?;
    let (video_tfdt_offset, video_tfdt_v1) =
        video_tfdt.ok_or_else(|| "Missing video tfdt in segment".to_string())?;
    let (audio_tfdt_offset, audio_tfdt_v1) =
        audio_tfdt.ok_or_else(|| "Missing audio tfdt in segment".to_string())?;

    Ok(SegmentTemplate {
        raw,
        mfhd_seq_offset,
        video_tfdt_offset,
        video_tfdt_v1,
        audio_tfdt_offset,
        audio_tfdt_v1,
    })
}

fn patch_init_with_mehd(init_bytes: &[u8], total_fragments: u64) -> Result<Vec<u8>, String> {
    let top = parse_boxes(init_bytes, 0, init_bytes.len())?;
    if top.len() < 2 || &top[0].0 != b"ftyp" || &top[1].0 != b"moov" {
        return Err("init.mp4 must begin with ftyp + moov".to_string());
    }

    let ftyp = &init_bytes[top[0].1..top[0].1 + top[0].2];
    let moov_start = top[1].1;
    let moov_end = moov_start + top[1].2;
    let moov_children = parse_boxes(init_bytes, moov_start + 8, moov_end)?;

    let mut mvhd_timescale: Option<u32> = None;
    for (ctyp, cpos, csz) in &moov_children {
        if ctyp == b"mvhd" {
            let ver = init_bytes[cpos + 8];
            let ts_off = if ver == 0 { cpos + 20 } else { cpos + 28 };
            if ts_off + 4 > cpos + csz {
                return Err("mvhd box truncated".to_string());
            }
            mvhd_timescale = Some(u32::from_be_bytes(
                init_bytes[ts_off..ts_off + 4].try_into().unwrap(),
            ));
            break;
        }
    }
    let mvhd_timescale = mvhd_timescale.ok_or_else(|| "Missing mvhd in init.mp4".to_string())?;
    let mehd_duration = ((total_fragments as u128) * 64 * (mvhd_timescale as u128) / 30) as u64;

    let mut moov_inner = Vec::with_capacity(top[1].2 + 32);
    let mut saw_mvex = false;
    for (ctyp, cpos, csz) in moov_children {
        if &ctyp == b"mvex" {
            saw_mvex = true;
            let mut mvex_inner = Vec::with_capacity(csz + 24);
            mvex_inner.extend_from_slice(&20u32.to_be_bytes());
            mvex_inner.extend_from_slice(b"mehd");
            mvex_inner.extend_from_slice(&0x0100_0000u32.to_be_bytes());
            mvex_inner.extend_from_slice(&mehd_duration.to_be_bytes());

            for (mtyp, mpos, msz) in parse_boxes(init_bytes, cpos + 8, cpos + csz)? {
                if &mtyp != b"mehd" {
                    mvex_inner.extend_from_slice(&init_bytes[mpos..mpos + msz]);
                }
            }
            moov_inner.extend_from_slice(&((8 + mvex_inner.len()) as u32).to_be_bytes());
            moov_inner.extend_from_slice(b"mvex");
            moov_inner.extend_from_slice(&mvex_inner);
        } else {
            moov_inner.extend_from_slice(&init_bytes[cpos..cpos + csz]);
        }
    }

    if !saw_mvex {
        return Err("Missing mvex in init.mp4".to_string());
    }

    let mut out = Vec::with_capacity(ftyp.len() + 8 + moov_inner.len());
    out.extend_from_slice(ftyp);
    out.extend_from_slice(&((8 + moov_inner.len()) as u32).to_be_bytes());
    out.extend_from_slice(b"moov");
    out.extend_from_slice(&moov_inner);
    Ok(out)
}

fn build_sidx(total_fragments: u64, slot_size: u64) -> Result<Vec<u8>, String> {
    let ref_count = u16::try_from(total_fragments)
        .map_err(|_| "total_fragments exceeds sidx u16 reference_count limit".to_string())?;
    if slot_size > 0x7FFF_FFFF {
        return Err("slot_size exceeds 31-bit sidx referenced_size limit".to_string());
    }
    let box_len = 40 + 12 * (ref_count as usize);
    let mut sidx = Vec::with_capacity(box_len);
    sidx.extend_from_slice(&(box_len as u32).to_be_bytes());
    sidx.extend_from_slice(b"sidx");
    sidx.extend_from_slice(&0x0100_0000u32.to_be_bytes()); // version=1, flags=0
    sidx.extend_from_slice(&VIDEO_TRACK_ID.to_be_bytes());
    sidx.extend_from_slice(&VIDEO_TIMESCALE.to_be_bytes());
    sidx.extend_from_slice(&0u64.to_be_bytes()); // earliest_presentation_time
    sidx.extend_from_slice(&0u64.to_be_bytes()); // first_offset
    sidx.extend_from_slice(&0u16.to_be_bytes()); // reserved
    sidx.extend_from_slice(&ref_count.to_be_bytes());

    let ref_word = (slot_size as u32) & 0x7FFF_FFFF;
    let dur_word = VIDEO_SEG_DURATION as u32;
    let sap_word = 0x9000_0000u32; // starts_with_SAP=1, SAP_type=1

    for _ in 0..ref_count {
        sidx.extend_from_slice(&ref_word.to_be_bytes());
        sidx.extend_from_slice(&dur_word.to_be_bytes());
        sidx.extend_from_slice(&sap_word.to_be_bytes());
    }
    Ok(sidx)
}

fn write_tfdt(buf: &mut [u8], offset: usize, is_v1: bool, value: u64) {
    if is_v1 {
        buf[offset..offset + 8].copy_from_slice(&value.to_be_bytes());
    } else {
        buf[offset..offset + 4].copy_from_slice(&(value as u32).to_be_bytes());
    }
}

impl PlaceholderBank {
    /// Loads `init.mp4` and all 16 × 7 `.m4s` placeholder segments and precomputes the virtual fMP4 layout.
    pub fn load(dir: &Path, duration_hours: u64) -> Result<Self, String> {
        let init_path = dir.join("init.mp4");
        let init_bytes = std::fs::read(&init_path)
            .map_err(|e| format!("Failed to read {:?}: {}", init_path, e))?;

        let mut states = HashMap::with_capacity(16);
        let mut max_seg_len = 0usize;

        for state in PlaceholderState::all() {
            let state_dir = dir.join(state.id());
            let mut templates: Vec<SegmentTemplate> = Vec::with_capacity(SEGMENTS_PER_LOOP);
            for idx in 0..SEGMENTS_PER_LOOP {
                let seg_path = state_dir.join(format!("seg_{}.m4s", idx));
                let raw = std::fs::read(&seg_path)
                    .map_err(|e| format!("Failed to read {:?}: {}", seg_path, e))?;
                max_seg_len = max_seg_len.max(raw.len());
                templates.push(parse_segment_template(raw)?);
            }
            let arr: [SegmentTemplate; SEGMENTS_PER_LOOP] = templates
                .try_into()
                .map_err(|_| "Expected exactly 7 segments per state".to_string())?;
            states.insert(state, arr);
        }

        let hours = duration_hours.clamp(1, 12);
        let total_fragments = ((hours * 3600 * 30).div_ceil(64)).min(u16::MAX as u64);
        // Reserve >= 8 bytes so any padding fits a valid ISO BMFF `free` box.
        let slot_size = ((max_seg_len as u64 + 8) + 4095) & !4095;

        let mut header_vec = patch_init_with_mehd(&init_bytes, total_fragments)?;
        let sidx = build_sidx(total_fragments, slot_size)?;
        header_vec.extend_from_slice(&sidx);
        let header = Bytes::from(header_vec);
        let total_len = header.len() as u64 + total_fragments * slot_size;

        Ok(Self {
            header,
            slot_size,
            total_fragments,
            total_len,
            states,
        })
    }

    /// Materializes a single fixed-slot `[moof][mdat][free?]` fragment for `frag_idx`.
    pub fn build_fragment(&self, state: PlaceholderState, frag_idx: u64) -> Vec<u8> {
        let seg_idx = (frag_idx % SEGMENTS_PER_LOOP as u64) as usize;
        let seg = &self
            .states
            .get(&state)
            .or_else(|| self.states.get(&PlaceholderState::Buffering(0)))
            .expect("PlaceholderBank always contains Buffering(0)")[seg_idx];

        let slot_usize = self.slot_size as usize;
        let mut buf = Vec::with_capacity(slot_usize);
        buf.extend_from_slice(&seg.raw);

        let seq_num = ((frag_idx + 1) as u32).to_be_bytes();
        buf[seg.mfhd_seq_offset..seg.mfhd_seq_offset + 4].copy_from_slice(&seq_num);

        write_tfdt(
            &mut buf,
            seg.video_tfdt_offset,
            seg.video_tfdt_v1,
            frag_idx * VIDEO_SEG_DURATION,
        );
        write_tfdt(
            &mut buf,
            seg.audio_tfdt_offset,
            seg.audio_tfdt_v1,
            frag_idx * AUDIO_SEG_DURATION,
        );

        let rem = slot_usize - buf.len();
        if rem > 0 {
            debug_assert!(rem >= 8);
            buf.extend_from_slice(&(rem as u32).to_be_bytes());
            buf.extend_from_slice(b"free");
            buf.resize(slot_usize, 0);
        }

        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::placeholder::DEFAULT_DURATION_HOURS;

    #[test]
    fn loads_all_generated_placeholder_states_and_segments() {
        let bank = PlaceholderBank::load(Path::new("placeholders"), DEFAULT_DURATION_HOURS)
            .expect("should load generated placeholders");
        let expected_fragments =
            ((DEFAULT_DURATION_HOURS.clamp(1, 12) * 3600 * 30).div_ceil(64)).min(u16::MAX as u64);
        assert_eq!(bank.states.len(), 16);
        assert_eq!(bank.total_fragments, expected_fragments);
        assert_eq!(bank.slot_size % 4096, 0);
        assert_eq!(
            bank.total_len,
            bank.header.len() as u64 + expected_fragments * bank.slot_size
        );
    }

    #[test]
    fn spliced_multi_state_virtual_mp4_has_valid_boxes() {
        let bank = PlaceholderBank::load(Path::new("placeholders"), DEFAULT_DURATION_HOURS)
            .expect("should load generated placeholders");

        let sequence = [
            PlaceholderState::DownloadStarted,
            PlaceholderState::DownloadStarted,
            PlaceholderState::FetchingMetadata,
            PlaceholderState::Buffering(0),
            PlaceholderState::Buffering(1),
            PlaceholderState::Buffering(2),
            PlaceholderState::Buffering(3),
            PlaceholderState::Buffering(4),
            PlaceholderState::Buffering(5),
            PlaceholderState::Buffering(6),
            PlaceholderState::Buffering(7),
            PlaceholderState::Buffering(8),
            PlaceholderState::Buffering(9),
            PlaceholderState::Stalled,
            PlaceholderState::FilesMissing,
            PlaceholderState::Ready,
            PlaceholderState::Ready,
        ];

        let mut blob = Vec::new();
        blob.extend_from_slice(&bank.header);
        for (idx, state) in sequence.iter().enumerate() {
            let frag = bank.build_fragment(*state, idx as u64);
            assert_eq!(frag.len(), bank.slot_size as usize);
            blob.extend_from_slice(&frag);
        }

        let boxes = parse_boxes(&blob, 0, blob.len()).expect("valid top-level boxes");
        assert_eq!(&boxes[0].0, b"ftyp");
        assert_eq!(&boxes[1].0, b"moov");
        assert_eq!(&boxes[2].0, b"sidx");

        let mut pos = 3;
        for frag_idx in 0..sequence.len() {
            assert_eq!(&boxes[pos].0, b"moof");
            let moof_pos = boxes[pos].1;
            let moof_sz = boxes[pos].2;
            let tpl = parse_segment_template(
                blob[moof_pos..moof_pos + boxes[pos].2 + boxes[pos + 1].2].to_vec(),
            )
            .expect("valid moof+mdat");
            let seq = u32::from_be_bytes(
                tpl.raw[tpl.mfhd_seq_offset..tpl.mfhd_seq_offset + 4]
                    .try_into()
                    .unwrap(),
            );
            assert_eq!(seq, (frag_idx + 1) as u32);
            let v_tfdt = if tpl.video_tfdt_v1 {
                u64::from_be_bytes(
                    tpl.raw[tpl.video_tfdt_offset..tpl.video_tfdt_offset + 8]
                        .try_into()
                        .unwrap(),
                )
            } else {
                u32::from_be_bytes(
                    tpl.raw[tpl.video_tfdt_offset..tpl.video_tfdt_offset + 4]
                        .try_into()
                        .unwrap(),
                ) as u64
            };
            assert_eq!(v_tfdt, (frag_idx as u64) * VIDEO_SEG_DURATION);

            assert_eq!(&boxes[pos + 1].0, b"mdat");
            let slot_consumed = moof_sz + boxes[pos + 1].2;
            if slot_consumed < bank.slot_size as usize {
                assert_eq!(&boxes[pos + 2].0, b"free");
                assert_eq!(slot_consumed + boxes[pos + 2].2, bank.slot_size as usize);
                pos += 3;
            } else {
                pos += 2;
            }
        }
        assert_eq!(pos, boxes.len());
    }
}
