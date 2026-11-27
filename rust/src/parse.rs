//! ffmpeg silencedetect 出力のパーサ。
//!
//! Delphi実験版の `SilenceParse.inc` をそのまま Rust に移植したもの。
//! 本体（GUI）と単体テストが同じコードを通るよう、純粋関数だけにしている。
//!
//! ffmpeg の silencedetect は次のような行を出す:
//!   [silencedetect @ 0000...] silence_start: 5.023000
//!   [silencedetect @ 0000...] silence_end: 9.011000 | silence_duration: 3.988000
//! ログ全体を走査して "silence_start:" と "silence_end:" を出現順に拾い、
//! 対にしていく。ファイル末尾まで続く無音（対にならない末尾の
//! silence_start）は無視する。ロケール非依存（小数点は '.' 固定）。

/// 無音区間（秒）。
pub struct SilenceRange {
    pub start_sec: f64,
    pub end_sec: f64,
}

const MARK_START: &[u8] = b"silence_start:";
const MARK_END: &[u8] = b"silence_end:";

/// `from` 以降で `needle` が最初に現れる位置（PosEx 相当）。
fn find_from(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from > haystack.len() || needle.is_empty() || haystack.len() - from < needle.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| from + p)
}

/// TryParseSecondsAt 相当: 空白を飛ばし、[0-9 . - +] を読み取って数値化する。
fn try_parse_seconds_at(s: &[u8], start: usize) -> Option<f64> {
    let mut i = start;
    while i < s.len() && s[i] == b' ' {
        i += 1;
    }
    let num_start = i;
    while i < s.len()
        && (s[i].is_ascii_digit() || s[i] == b'.' || s[i] == b'-' || s[i] == b'+')
    {
        i += 1;
    }
    if i == num_start {
        return None;
    }
    let text = std::str::from_utf8(&s[num_start..i]).ok()?;
    text.parse::<f64>().ok()
}

/// ParseSilenceDetectLog 相当。対になった区間だけを出現順に返す。
pub fn parse_silence_detect_log(log: &[u8]) -> Vec<SilenceRange> {
    let mut ranges: Vec<SilenceRange> = Vec::new();
    let mut offset: usize = 0;
    let mut pending_start: Option<f64> = None;
    loop {
        let p_s = find_from(log, MARK_START, offset);
        let p_e = find_from(log, MARK_END, offset);
        if p_s.is_none() && p_e.is_none() {
            break;
        }
        // end マーカーが先に来る場合（start が無い、または end の方が前）
        let end_first = match (p_s, p_e) {
            (Some(s), Some(e)) => e < s,
            (None, Some(_)) => true,
            _ => false,
        };
        if end_first {
            let e = p_e.unwrap();
            if let Some(start) = pending_start {
                if let Some(v) = try_parse_seconds_at(log, e + MARK_END.len()) {
                    ranges.push(SilenceRange {
                        start_sec: start,
                        end_sec: v,
                    });
                    pending_start = None;
                }
            }
            offset = e + MARK_END.len();
        } else {
            let s = p_s.unwrap();
            if let Some(v) = try_parse_seconds_at(log, s + MARK_START.len()) {
                pending_start = Some(v);
            }
            offset = s + MARK_START.len();
        }
    }
    ranges
}

/// FmtSec 相当: FormatFloat('0.000', x)（小数点は '.' 固定）。
pub fn fmt_sec(x: f64) -> String {
    format!("{:.3}", x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.001
    }

    /// Delphi版 TestParse.pas の合成ログテストと同一内容・同一期待値。
    #[test]
    fn synthetic_log() {
        let log: &[u8] = b"[silencedetect @ 000001] silence_start: 1.500000\n\
            some unrelated ffmpeg chatter\n\
            [silencedetect @ 000001] silence_end: 3.250000 | silence_duration: 1.750000\n\
            [silencedetect @ 000001] silence_start: 10.000000\n\
            [silencedetect @ 000001] silence_end: 12.500000 | silence_duration: 2.500000\n\
            [silencedetect @ 000001] silence_start: 99.000000\n";
        let ranges = parse_silence_detect_log(log);
        assert_eq!(ranges.len(), 2, "expected 2 ranges");
        assert!(
            near(ranges[0].start_sec, 1.5) && near(ranges[0].end_sec, 3.25),
            "range 1 mismatch: {} - {}",
            ranges[0].start_sec,
            ranges[0].end_sec
        );
        assert!(
            near(ranges[1].start_sec, 10.0) && near(ranges[1].end_sec, 12.5),
            "range 2 mismatch: {} - {}",
            ranges[1].start_sec,
            ranges[1].end_sec
        );
    }

    /// Delphi版の実ログテストと同一ファイル・同一期待値
    /// （test_silence.wav を実 ffmpeg で処理した stderr）。
    #[test]
    fn real_ffmpeg_log() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../mp3-silence-mini-delphi/ffmpeg_log.txt"
        );
        let log = std::fs::read(path).expect("ffmpeg_log.txt が読めません");
        let ranges = parse_silence_detect_log(&log);
        assert_eq!(ranges.len(), 2, "expected 2 ranges");
        assert!(
            near(ranges[0].start_sec, 2.0) && near(ranges[0].end_sec, 5.0),
            "range 1 mismatch: {} - {}",
            ranges[0].start_sec,
            ranges[0].end_sec
        );
        assert!(
            near(ranges[1].start_sec, 7.0) && near(ranges[1].end_sec, 9.0),
            "range 2 mismatch: {} - {}",
            ranges[1].start_sec,
            ranges[1].end_sec
        );
    }

    #[test]
    fn fmt_sec_matches_delphi() {
        assert_eq!(fmt_sec(0.0), "0.000");
        assert_eq!(fmt_sec(4.949), "4.949");
        assert_eq!(fmt_sec(1224.913), "1224.913");
    }
}
