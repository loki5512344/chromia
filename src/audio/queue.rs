//! Playback queue with shuffle and repeat support.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::audio::RepeatMode;
use crate::library::Track;

/// A tiny xorshift64 PRNG used to build shuffle orders without pulling in a
/// dependency. Seeded from the system clock so consecutive launches differ.
struct XorShift {
    state: u64,
}

impl XorShift {
    fn from_time() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        let state = if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        };
        Self { state }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        (x >> 32) as u32
    }
}

/// A playable track list with shuffle and repeat handling.
pub struct Queue {
    tracks: Vec<Track>,
    current: Option<usize>,
    shuffle: bool,
    repeat: RepeatMode,
    order: Vec<usize>,
    order_pos: usize,
    at_start: bool,
    rand: XorShift,
}

impl Queue {
    /// Creates an empty queue with repeat disabled and shuffle off.
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            current: None,
            shuffle: false,
            repeat: RepeatMode::Off,
            order: Vec::new(),
            order_pos: 0,
            at_start: true,
            rand: XorShift::from_time(),
        }
    }

    /// Replaces the queue contents. The currently playing track is kept at its
    /// new position if it is still present, otherwise playback stops.
    pub fn load(&mut self, tracks: Vec<Track>) {
        let current_track = self.current();
        self.tracks = tracks;
        self.current = match current_track {
            Some(track) => self.tracks.iter().position(|t| *t == track),
            None => None,
        };
        self.refresh_order();
    }

    /// Selects the track at `index` as the current one and returns it, or
    /// `None` if the index is out of bounds.
    pub fn play_at(&mut self, index: usize) -> Option<Track> {
        if index >= self.tracks.len() {
            return None;
        }
        self.current = Some(index);
        if self.shuffle {
            self.rebuild_order_from(index);
        }
        self.tracks.get(index).cloned()
    }

    /// Updates the resolved file path of the track at `index` in place.
    ///
    /// Used by the player to persist a downloaded stream path so it is not
    /// re-downloaded on the next pass through the queue.
    pub fn set_track_path(&mut self, index: usize, path: PathBuf) {
        if let Some(track) = self.tracks.get_mut(index) {
            track.path = path;
        }
    }

    /// Index of the current track, if any.
    pub fn current_index(&self) -> Option<usize> {
        self.current
    }

    /// A clone of the current track, if any.
    pub fn current(&self) -> Option<Track> {
        self.current
            .and_then(|index| self.tracks.get(index))
            .cloned()
    }

    /// Number of tracks in the queue.
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    /// Whether the queue holds no tracks.
    #[allow(dead_code)] // TODO(loki): used by queue editing UI
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// Index of the track to advance to, honouring shuffle and repeat rules.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<usize> {
        if self.tracks.is_empty() {
            return None;
        }
        if self.shuffle {
            if self.repeat == RepeatMode::One {
                return self.current;
            }
            if self.order.is_empty() {
                self.rebuild_order();
            }
            if self.at_start {
                self.at_start = false;
                return Some(self.order[0]);
            }
            if self.order_pos + 1 < self.order.len() {
                self.order_pos += 1;
                return Some(self.order[self.order_pos]);
            }
            if self.repeat == RepeatMode::All {
                self.rebuild_order();
                if self.order_pos + 1 < self.order.len() {
                    self.order_pos += 1;
                    return Some(self.order[self.order_pos]);
                }
                self.order_pos = 0;
                return Some(self.order[0]);
            }
            None
        } else {
            let current = self.current?;
            match self.repeat {
                RepeatMode::One => Some(current),
                RepeatMode::Off | RepeatMode::All => {
                    if current + 1 < self.tracks.len() {
                        Some(current + 1)
                    } else if self.repeat == RepeatMode::All {
                        Some(0)
                    } else {
                        None
                    }
                }
            }
        }
    }

    /// Index of the track to go back to, honouring shuffle and repeat rules.
    pub fn previous(&mut self) -> Option<usize> {
        if self.tracks.is_empty() {
            return None;
        }
        if self.shuffle {
            if self.order.is_empty() {
                self.rebuild_order();
            }
            if self.at_start {
                return Some(self.order[0]);
            }
            if self.order_pos > 0 {
                self.order_pos -= 1;
                return Some(self.order[self.order_pos]);
            }
            if self.repeat == RepeatMode::All {
                self.order_pos = self.order.len() - 1;
                return Some(self.order[self.order_pos]);
            }
            Some(self.order[0])
        } else {
            let current = self.current?;
            if current > 0 {
                Some(current - 1)
            } else if self.repeat == RepeatMode::All {
                Some(self.tracks.len() - 1)
            } else {
                Some(0)
            }
        }
    }

    /// Appends a track to the end of the queue.
    #[allow(dead_code)] // TODO(loki): used by queue editing UI
    pub fn add(&mut self, track: Track) {
        self.tracks.push(track);
        if self.shuffle {
            self.rebuild_order();
        }
    }

    /// Removes and returns the track at `index`, adjusting the current index.
    #[allow(dead_code)] // TODO(loki): used by queue editing UI
    pub fn remove_at(&mut self, index: usize) -> Option<Track> {
        if index >= self.tracks.len() {
            return None;
        }
        let removed = self.tracks.remove(index);
        if let Some(current) = self.current {
            if current == index {
                self.current = None;
            } else if current > index {
                self.current = Some(current - 1);
            }
        }
        self.refresh_order();
        Some(removed)
    }

    /// Empties the queue.
    #[allow(dead_code)] // TODO(loki): used by queue editing UI
    pub fn clear(&mut self) {
        self.tracks.clear();
        self.current = None;
        self.order.clear();
        self.order_pos = 0;
        self.at_start = true;
    }

    /// Enables or disables shuffle.
    pub fn set_shuffle(&mut self, shuffle: bool) {
        self.shuffle = shuffle;
        if shuffle {
            self.rebuild_order();
        }
    }

    /// Whether shuffle is enabled.
    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    /// Sets the repeat mode.
    pub fn set_repeat(&mut self, mode: RepeatMode) {
        self.repeat = mode;
    }

    /// The current repeat mode.
    pub fn repeat(&self) -> RepeatMode {
        self.repeat
    }

    /// Snapshot of all tracks in play order.
    pub fn tracks(&self) -> Vec<Track> {
        self.tracks.clone()
    }

    /// Regenerates the shuffle order and repositions it on the current track.
    fn rebuild_order(&mut self) {
        let mut order: Vec<usize> = (0..self.tracks.len()).collect();
        for i in (1..order.len()).rev() {
            let j = (self.rand.next_u32() as usize) % (i + 1);
            order.swap(i, j);
        }
        self.order = order;
        self.order_pos = match self.current {
            Some(c) => self.order.iter().position(|&x| x == c).unwrap_or(0),
            None => 0,
        };
        self.at_start = self.current.is_none();
    }

    /// Builds a fresh shuffle order starting from the given index.
    fn rebuild_order_from(&mut self, start: usize) {
        let mut order: Vec<usize> = (0..self.tracks.len()).collect();
        for i in (1..order.len()).rev() {
            let j = (self.rand.next_u32() as usize) % (i + 1);
            order.swap(i, j);
        }
        if let Some(pos) = order.iter().position(|&x| x == start) {
            order.rotate_left(pos);
        }
        self.order = order;
        self.order_pos = 0;
        self.at_start = false;
    }

    fn refresh_order(&mut self) {
        if self.shuffle {
            self.rebuild_order();
        } else {
            self.order.clear();
            self.order_pos = 0;
            self.at_start = self.current.is_none();
        }
    }
}

impl Default for Queue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::time::Duration;

    use super::Queue;
    use crate::audio::RepeatMode;
    use crate::library::Track;

    fn track(id: i64) -> Track {
        let mut track = Track::new_local(
            format!("/tmp/track-{id}.mp3").into(),
            format!("Title {id}"),
            "Artist".into(),
            "Album".into(),
            Duration::from_secs(180),
        );
        track.id = id;
        track
    }

    #[test]
    fn shuffle_never_repeats_within_a_pass() {
        let mut queue = Queue::new();
        queue.load((0..10).map(track).collect());
        queue.set_shuffle(true);
        let mut seen = HashSet::new();
        let mut count = 0;
        while let Some(index) = queue.next() {
            assert!(index < 10, "shuffle index out of range: {index}");
            assert!(seen.insert(index), "shuffle repeated index {index}");
            count += 1;
        }
        assert_eq!(count, 10);
    }

    #[test]
    fn next_repeat_off_stops_at_end() {
        let mut queue = Queue::new();
        queue.load(vec![track(0), track(1), track(2)]);
        assert_eq!(queue.play_at(0).map(|t| t.id), Some(0));
        assert_eq!(queue.next(), Some(1));
        queue.play_at(1);
        assert_eq!(queue.next(), Some(2));
        queue.play_at(2);
        assert_eq!(queue.next(), None);
    }

    #[test]
    fn next_repeat_all_wraps_to_start() {
        let mut queue = Queue::new();
        queue.load(vec![track(0), track(1), track(2)]);
        queue.set_repeat(RepeatMode::All);
        assert_eq!(queue.play_at(2).map(|t| t.id), Some(2));
        assert_eq!(queue.next(), Some(0));
        queue.play_at(0);
        assert_eq!(queue.next(), Some(1));
    }

    #[test]
    fn next_repeat_one_stays_on_current() {
        let mut queue = Queue::new();
        queue.load(vec![track(0), track(1), track(2)]);
        queue.set_repeat(RepeatMode::One);
        assert_eq!(queue.play_at(1).map(|t| t.id), Some(1));
        assert_eq!(queue.next(), Some(1));
    }

    #[test]
    fn previous_walks_back_and_clamps() {
        let mut queue = Queue::new();
        queue.load(vec![track(0), track(1), track(2)]);
        assert_eq!(queue.play_at(2).map(|t| t.id), Some(2));
        assert_eq!(queue.previous(), Some(1));
        queue.play_at(1);
        assert_eq!(queue.previous(), Some(0));
        queue.play_at(0);
        assert_eq!(queue.previous(), Some(0));

        queue.set_repeat(RepeatMode::All);
        assert_eq!(queue.previous(), Some(2));
    }

    #[test]
    fn remove_at_adjusts_current_index() {
        let mut queue = Queue::new();
        queue.load(vec![track(0), track(1), track(2), track(3)]);
        assert_eq!(queue.play_at(2).map(|t| t.id), Some(2));
        assert_eq!(queue.remove_at(0).map(|t| t.id), Some(0));
        assert_eq!(queue.current_index(), Some(1));
        assert_eq!(queue.remove_at(1).map(|t| t.id), Some(2));
        assert_eq!(queue.current_index(), None);
    }

    #[test]
    fn play_at_returns_correct_track() {
        let mut queue = Queue::new();
        queue.load(vec![track(0), track(1), track(2)]);
        assert_eq!(queue.play_at(1).map(|t| t.id), Some(1));
        assert_eq!(queue.current().map(|t| t.id), Some(1));
        assert_eq!(queue.play_at(99), None);
        assert_eq!(queue.current_index(), Some(1));
    }

    #[test]
    fn load_keeps_current_track_when_present() {
        let mut queue = Queue::new();
        queue.load(vec![track(0), track(1)]);
        assert_eq!(queue.play_at(0).map(|t| t.id), Some(0));
        queue.load(vec![track(9), track(0), track(1)]);
        assert_eq!(queue.current_index(), Some(1));
        assert_eq!(queue.current().map(|t| t.id), Some(0));
    }
}
