//! LRC (`.lrc`) lyrics parsing and the lrclib.net API client.

use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use reqwest::StatusCode;
use serde_json::Value;

/// A single timestamped lyric line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricLine {
    /// Offset from the start of the track.
    pub time: Duration,
    /// The lyric text.
    pub text: String,
}

/// Lyrics for a track, either synced (`lines`) or plain text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lyrics {
    /// Timestamped lines; plain lyrics are a single line at time zero.
    pub lines: Vec<LyricLine>,
    /// The plain-text lyric blob, when the server only provides that.
    pub plain: Option<String>,
}

/// Client for the lrclib.net lyrics API.
#[derive(Debug, Clone)]
pub struct Lrclib {
    client: reqwest::Client,
}

impl Lrclib {
    /// Creates a new client.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Fetches lyrics for a track.
    ///
    /// Returns `Ok(None)` when lrclib has no record of the track (HTTP 404);
    /// any other failure is returned as an error.
    pub async fn get_lyrics(
        &self,
        artist: &str,
        title: &str,
        album: &str,
        duration: Duration,
    ) -> Result<Option<Lyrics>> {
        let seconds = duration.as_secs().to_string();
        let url = reqwest::Url::parse_with_params(
            "https://lrclib.net/api/get",
            &[
                ("artist", artist),
                ("track_name", title),
                ("album_name", album),
                ("duration", seconds.as_str()),
            ],
        )
        .context("failed to build the lrclib request URL")?;

        let response = self
            .client
            .get(url)
            .header(reqwest::header::USER_AGENT, "chromia/0.1.0")
            .send()
            .await
            .context("failed to reach lrclib.net")?;

        match response.status() {
            StatusCode::NOT_FOUND => return Ok(None),
            StatusCode::OK => {}
            status => return Err(anyhow!("lrclib.net responded with status {status}")),
        }

        let body: Value = response
            .json()
            .await
            .context("failed to parse lrclib response")?;
        let synced = body.get("syncedLyrics").and_then(Value::as_str);
        let plain = body.get("plainLyrics").and_then(Value::as_str);

        match synced {
            Some(lrc) => Ok(Some(Lyrics {
                lines: parse_lrc(lrc),
                plain: plain.map(str::to_string),
            })),
            None => match plain {
                Some(text) => Ok(Some(Lyrics {
                    lines: vec![LyricLine {
                        time: Duration::ZERO,
                        text: text.to_string(),
                    }],
                    plain: Some(text.to_string()),
                })),
                None => Ok(None),
            },
        }
    }
}

impl Default for Lrclib {
    fn default() -> Self {
        Self::new()
    }
}

/// Parses LRC-formatted text into timestamped lines, sorted by time.
///
/// Handles `[mm:ss.xx]` and `[mm:ss]` timestamps, multiple timestamps per
/// line, and skips metadata tags such as `[ar:...]` and `[ti:...]`.
pub fn parse_lrc(input: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();
    for raw in input.lines() {
        let (times, text) = extract_timestamps(raw);
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        for time in times {
            lines.push(LyricLine {
                time,
                text: text.to_string(),
            });
        }
    }
    lines.sort_by(|a, b| a.time.cmp(&b.time).then_with(|| a.text.cmp(&b.text)));
    lines
}

/// Splits a line into its leading `[mm:ss(.xx)]` timestamps and remaining text.
fn extract_timestamps(line: &str) -> (Vec<Duration>, &str) {
    let mut times = Vec::new();
    let mut rest = line;
    while let Some(tail) = rest.strip_prefix('[') {
        let Some(end) = tail.find(']') else {
            break;
        };
        let Some(time) = parse_timestamp(&tail[..end]) else {
            break;
        };
        times.push(time);
        rest = &tail[end + 1..];
    }
    (times, rest)
}

/// Parses a single `[mm:ss(.xx)]` timestamp; `None` for metadata tags.
fn parse_timestamp(token: &str) -> Option<Duration> {
    let (minutes, seconds) = token.split_once(':')?;
    let minutes: u64 = minutes.trim().parse().ok()?;
    let (whole, fraction) = match seconds.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => (seconds, ""),
    };
    let whole: u64 = whole.trim().parse().ok()?;
    Some(Duration::from_millis(
        minutes.saturating_mul(60_000) + whole.saturating_mul(1_000) + parse_fraction_ms(fraction),
    ))
}

/// Interprets a fractional-seconds string (hundredths or thousandths) as ms.
fn parse_fraction_ms(fraction: &str) -> u64 {
    let mut digits: Vec<u8> = fraction
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .map(|ch| ch as u8 - b'0')
        .collect();
    digits.truncate(3);
    while digits.len() < 3 {
        digits.push(0);
    }
    digits
        .into_iter()
        .fold(0, |acc, digit| acc * 10 + u64::from(digit))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::parse_lrc;

    #[test]
    fn parses_realistic_lrc() {
        let lrc = concat!(
            "[ti:Never Gonna Give You Up]\n",
            "[ar:Rick Astley]\n",
            "[al:Whenever You Need Somebody]\n",
            "[length:03:33]\n",
            "[00:12.00]We're no strangers to love\n",
            "[00:15.50]You know the rules and so do I\n",
            "[00:18.20][00:24.00]A full commitment's what I'm thinking of\n",
            "[00:27.50]You wouldn't get this from any other guy\n",
            "[01:02]I just wanna tell you how I'm feeling\n",
        );
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 6);

        let first = &lines[0];
        assert_eq!(first.time, Duration::from_millis(12_000));
        assert_eq!(first.text, "We're no strangers to love");

        assert_eq!(lines[2].time, Duration::from_millis(18_200));
        assert_eq!(lines[2].text, "A full commitment's what I'm thinking of");
        assert_eq!(lines[3].time, Duration::from_millis(24_000));
        assert_eq!(lines[3].text, "A full commitment's what I'm thinking of");
        assert_eq!(lines[5].time, Duration::from_secs(62));
        assert_eq!(lines[5].text, "I just wanna tell you how I'm feeling");
    }

    #[test]
    fn ignores_metadata_and_blank_lines() {
        let lrc = "[ar:Some Artist]\n\n[00:01.00]First line\n";
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].time, Duration::from_millis(1_000));
        assert_eq!(lines[0].text, "First line");
    }
}
