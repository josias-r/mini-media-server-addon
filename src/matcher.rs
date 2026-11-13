use crate::parser::is_definitely_wrong_episode;
use strsim::levenshtein;

pub fn find_best_file_match(
    files: &[(&str, u64)],
    expected_title: &str,
    is_series: bool,
    season: u32,
    episode: u32,
) -> Option<usize> {
    let video_extensions = [".mp4", ".mkv", ".avi", ".webm", ".mov"];

    // 1. Filter out non-videos and wrong episodes
    let mut valid_files = Vec::new();
    for (idx, &(name, size)) in files.iter().enumerate() {
        let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
        let mut is_video = false;
        for vext in &video_extensions {
            if vext.ends_with(&ext) || format!(".{}", ext) == *vext {
                is_video = true;
                break;
            }
        }

        if is_video {
            if is_series && is_definitely_wrong_episode(name, season, episode) {
                continue;
            }
            valid_files.push((idx, name, size));
        }
    }

    // Fallback: If no files passed the episode filter, just take all videos
    if valid_files.is_empty() {
        for (idx, &(name, size)) in files.iter().enumerate() {
            let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
            for vext in &video_extensions {
                if vext.ends_with(&ext) || format!(".{}", ext) == *vext {
                    valid_files.push((idx, name, size));
                    break;
                }
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
        let filename_lower = name.to_lowercase();
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
