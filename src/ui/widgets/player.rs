//! Shared playback helpers (cover decoding, time formatting).
//!
//! The full transport UI now lives in `ui::bottom_player`; this module keeps the
//! small helpers used by several widgets.

use std::time::Duration;

/// Formats a [`Duration`] as `m:ss`.
pub(crate) fn fmt_duration(d: Duration) -> String {
    let total = d.as_secs();
    format!("{}:{:02}", total / 60, total % 60)
}

/// Builds a square `size`x`size` pixbuf from encoded cover-art bytes.
///
/// Returns `None` when the bytes cannot be decoded by GDK.
pub(crate) fn cover_pixbuf(bytes: &[u8], size: i32) -> Option<gtk::gdk_pixbuf::Pixbuf> {
    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_read(std::io::Cursor::new(bytes.to_vec())).ok()?;
    pixbuf.scale_simple(size, size, gtk::gdk_pixbuf::InterpType::Bilinear)
}

#[cfg(test)]
mod tests {
    use super::fmt_duration;
    use std::time::Duration;

    #[test]
    fn formats_mm_ss() {
        assert_eq!(fmt_duration(Duration::from_secs(0)), "0:00");
        assert_eq!(fmt_duration(Duration::from_secs(59)), "0:59");
        assert_eq!(fmt_duration(Duration::from_secs(60)), "1:00");
        assert_eq!(fmt_duration(Duration::from_secs(65)), "1:05");
        assert_eq!(fmt_duration(Duration::from_secs(600)), "10:00");
    }
}
