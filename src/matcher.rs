use crate::parser::{is_definitely_wrong_episode, is_video_file};
use strsim::levenshtein;

pub fn find_best_file_match(
    files: &[(&str, u64)],
    expected_title: &str,
    is_series: bool,
    season: u32,
    episode: u32,
) -> Option<usize> {
    // 1. Filter out non-videos and wrong episodes
    let mut valid_files = Vec::new();
    for (idx, &(name, size)) in files.iter().enumerate() {
        if is_video_file(name) {
            if is_series && is_definitely_wrong_episode(name, season, episode) {
                continue;
            }
            valid_files.push((idx, name, size));
        }
    }

    // Fallback: If no files passed the episode filter, just take all videos
    if valid_files.is_empty() {
        for (idx, &(name, size)) in files.iter().enumerate() {
            if is_video_file(name) {
                valid_files.push((idx, name, size));
            }
        }
    }

    if valid_files.is_empty() {
        return None;
    }

    // 2. Token + Levenshtein Matching
    let expected_title_lower = expected_title.to_lowercase();
    let expected_tokens: Vec<&str> = expected_title_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();

    let mut best_idx = 0;
    // We want the LOWEST distance score. And for ties, the LARGEST size.
    // We can store (distance_score, size).
    // To easily use `cmp`, since we want to MINIMIZE distance and MAXIMIZE size,
    // we can sort by distance ascending, then size descending.
    // Or keep a running best.
    let mut best_score: Option<(usize, u64)> = None;

    for (i, &(_idx, name, size)) in valid_files.iter().enumerate() {
        let basename = name.rsplit(['/', '\\']).next().unwrap_or(name);
        let filename_lower = basename.to_lowercase();
        let filename_tokens: Vec<&str> = filename_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();

        let mut total_distance = 0;

        for &expected_token in &expected_tokens {
            // Find the minimum Levenshtein distance from this expected token to any filename token
            let mut min_token_dist = usize::MAX;
            for &file_token in &filename_tokens {
                let dist = levenshtein(expected_token, file_token);
                if dist < min_token_dist {
                    min_token_dist = dist;
                }
            }
            // If filename has no tokens, min_token_dist remains MAX. We should fallback to the expected token length.
            if min_token_dist == usize::MAX {
                min_token_dist = expected_token.len();
            }
            total_distance += min_token_dist;
        }

        let current_score = (total_distance, size);

        match best_score {
            None => {
                best_score = Some(current_score);
                best_idx = i;
            }
            Some((best_dist, best_size)) => {
                // If total_distance is strictly less, it's better.
                // If it's equal, then we check if size is strictly greater.
                if total_distance < best_dist || (total_distance == best_dist && size > best_size) {
                    best_score = Some(current_score);
                    best_idx = i;
                }
            }
        }
    }

    let (chosen_idx, _, _) = valid_files[best_idx];
    Some(chosen_idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_requested_episode_from_avatar_season_pack_despite_smaller_size() {
        let files = vec![
            ("[TGx]Downloaded from torrentgalaxy.to .txt", 479),
            (
                "Avatar.The.Last.Airbender.S01.COMPLETE.1080p.NF.WEB-DL.DDP5.1.x264-LiTTLEBLUEMAN[TGx]/Avatar.The.Last.Airbender.S01E01.Aang.1080p.NF.WEB-DL.DDP5.1.x264-NTb.mkv",
                2_555_000_000,
            ),
            (
                "Avatar.The.Last.Airbender.S01.COMPLETE.1080p.NF.WEB-DL.DDP5.1.x264-LiTTLEBLUEMAN[TGx]/Avatar.The.Last.Airbender.S01E02.Warriors.1080p.NF.WEB-DL.DDP5.1.x264-NTb.mkv",
                1_900_000_000,
            ),
            (
                "Avatar.The.Last.Airbender.S01.COMPLETE.1080p.NF.WEB-DL.DDP5.1.x264-LiTTLEBLUEMAN[TGx]/Avatar.The.Last.Airbender.S01E08.Legends.1080p.NF.WEB-DL.DDP5.1.x264-NTb.mkv",
                3_414_000_000,
            ),
            ("NEW upcoming releases by Xclusive.txt", 71),
        ];

        let matched = find_best_file_match(&files, "Avatar: The Last Airbender S01E02", true, 1, 2);
        assert_eq!(matched, Some(2));
    }

    #[test]
    fn picks_requested_episode_from_bocchi_season_pack() {
        let files = vec![
            (
                "Bocchi.The.Rock.S01.1080p.BluRay.AV1.Opus.2.0-dAV1nci/Bocchi.The.Rock.S01E01.1080p.BluRay.AV1.Opus.2.0-dAV1nci.mkv",
                263_313_996,
            ),
            (
                "Bocchi.The.Rock.S01.1080p.BluRay.AV1.Opus.2.0-dAV1nci/Bocchi.The.Rock.S01E02.1080p.BluRay.AV1.Opus.2.0-dAV1nci.mkv",
                270_033_864,
            ),
            (
                "Bocchi.The.Rock.S01.1080p.BluRay.AV1.Opus.2.0-dAV1nci/Bocchi.The.Rock.S01E03.1080p.BluRay.AV1.Opus.2.0-dAV1nci.mkv",
                314_039_932,
            ),
        ];

        let matched = find_best_file_match(&files, "Bocchi the Rock! S01E02", true, 1, 2);
        assert_eq!(matched, Some(1));
    }

    #[test]
    fn picks_requested_episode_with_bare_numbering_and_folder_ranges() {
        let bare_files = vec![
            ("Show.S01/01 - Pilot.mkv", 500_000_000),
            ("Show.S01/02 - Warriors.mkv", 350_000_000),
            ("Show.S01/03 - Omashu.mkv", 450_000_000),
        ];
        assert_eq!(
            find_best_file_match(&bare_files, "Show S01E02", true, 1, 2),
            Some(1)
        );

        let folder_range_files = vec![
            ("Show.S01E01-E08/Show.S01E01.mkv", 500_000_000),
            ("Show.S01E01-E08/Show.S01E02.mkv", 350_000_000),
        ];
        assert_eq!(
            find_best_file_match(&folder_range_files, "Show S01E02", true, 1, 2),
            Some(1)
        );
    }
}
