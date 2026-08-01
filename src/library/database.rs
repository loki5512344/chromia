//! SQLite-backed persistent storage for [`Track`]s, playlists and history.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter};

use anyhow::Context;

use crate::library::{SourceKind, Track};

/// Column list shared by every `SELECT` statement.
const COLUMNS: &str = "id, source, path, url, thumbnail, title, artist, album, album_artist, \
                       duration_ms, track_no, disc_no, genre, year, bpm, play_count, last_played";

/// Column list qualified with `tracks.` for `SELECT`s that join other tables.
const TRACK_COLUMNS: &str = "tracks.id, tracks.source, tracks.path, tracks.url, \
                             tracks.thumbnail, tracks.title, tracks.artist, tracks.album, \
                             tracks.album_artist, tracks.duration_ms, tracks.track_no, \
                             tracks.disc_no, tracks.genre, tracks.year, tracks.bpm, \
                             tracks.play_count, tracks.last_played";

/// Database schema, created lazily on [`Database::open`].
///
/// Local tracks are keyed by `(source, path)`; remote tracks by `(source, url)`
/// so search results without a local file never collide.
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS tracks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source TEXT NOT NULL,
    path TEXT NOT NULL DEFAULT '',
    url TEXT,
    thumbnail TEXT,
    title TEXT NOT NULL,
    artist TEXT NOT NULL DEFAULT '',
    album TEXT NOT NULL DEFAULT '',
    album_artist TEXT NOT NULL DEFAULT '',
    duration_ms INTEGER NOT NULL DEFAULT 0,
    track_no INTEGER,
    disc_no INTEGER,
    genre TEXT,
    year INTEGER,
    bpm REAL,
    play_count INTEGER NOT NULL DEFAULT 0,
    last_played INTEGER
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_tracks_local_key ON tracks(source, path) WHERE source = 'local';
CREATE UNIQUE INDEX IF NOT EXISTS idx_tracks_remote_key ON tracks(source, url) WHERE source != 'local';
CREATE INDEX IF NOT EXISTS idx_tracks_title ON tracks(title);
CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist);
CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album);

CREATE TABLE IF NOT EXISTS playlists (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS playlist_tracks (
    playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    PRIMARY KEY (playlist_id, position)
);

CREATE TABLE IF NOT EXISTS history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    played_at INTEGER NOT NULL
);";

/// A named, ordered collection of tracks.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)] // TODO(loki): playlist management UI
pub struct Playlist {
    /// Playlist row id.
    pub id: i64,
    /// Display name.
    pub name: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// A handle to the track database.
///
/// The connection is wrapped in a [`Mutex`], making the handle `Send + Sync` so
/// it can be shared between threads via `Arc<Database>`.
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Opens (creating if necessary) the database at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error when the parent directory cannot be created, the file
    /// cannot be opened, or the schema cannot be initialised.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create database directory '{}'", parent.display())
            })?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open database at '{}'", path.display()))?;
        conn.execute_batch(SCHEMA)
            .context("failed to initialise database schema")?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Inserts `track` or updates the existing row with the same key (local
    /// tracks key on `path`, remote tracks on `url`).
    ///
    /// Sets `track.id` to the persisted row id and returns a clone of the track.
    ///
    /// # Errors
    ///
    /// Returns an error when the row cannot be written.
    pub fn upsert_track(&self, track: &Track) -> anyhow::Result<Track> {
        let mut track = track.clone();
        let conn = self.conn.lock();
        let existing_id: Option<i64> = match track.source {
            SourceKind::Local => conn
                .query_row(
                    "SELECT id FROM tracks WHERE source = ?1 AND path = ?2",
                    params![
                        track.source.to_string(),
                        track.path.to_string_lossy().to_string(),
                    ],
                    |row| row.get(0),
                )
                .optional()
                .context("failed to look up local track")?,
            SourceKind::Youtube | SourceKind::Soundcloud => conn
                .query_row(
                    "SELECT id FROM tracks WHERE source = ?1 AND url = ?2",
                    params![track.source.to_string(), &track.url],
                    |row| row.get(0),
                )
                .optional()
                .context("failed to look up remote track")?,
        };

        let source = track.source.to_string();
        let path = track.path.to_string_lossy().to_string();
        let duration = track.duration.as_millis() as i64;
        let last_played = track.last_played.map(|last| last.timestamp());
        let values: Vec<&dyn rusqlite::ToSql> = vec![
            &source,
            &path,
            &track.url,
            &track.thumbnail,
            &track.title,
            &track.artist,
            &track.album,
            &track.album_artist,
            &duration,
            &track.track_no,
            &track.disc_no,
            &track.genre,
            &track.year,
            &track.bpm,
            &track.play_count,
            &last_played,
        ];

        match existing_id {
            Some(id) => {
                let mut update: Vec<&dyn rusqlite::ToSql> = values.clone();
                update.push(&id);
                conn.execute(
                    "UPDATE tracks SET
                        path = ?2, url = ?3, thumbnail = ?4, title = ?5, artist = ?6,
                        album = ?7, album_artist = ?8, duration_ms = ?9, track_no = ?10,
                        disc_no = ?11, genre = ?12, year = ?13, bpm = ?14,
                        play_count = ?15, last_played = ?16
                     WHERE id = ?17",
                    update.as_slice(),
                )
                .context("failed to update track")?;
                track.id = id;
            }
            None => {
                conn.execute(
                    "INSERT INTO tracks (
                        source, path, url, thumbnail, title, artist, album, album_artist,
                        duration_ms, track_no, disc_no, genre, year, bpm, play_count, last_played
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                    values.as_slice(),
                )
                .context("failed to insert track")?;
                track.id = conn.last_insert_rowid();
            }
        }
        Ok(track)
    }

    /// Inserts or updates every track in `tracks`.
    ///
    /// Returns the number of processed tracks.
    ///
    /// # Errors
    ///
    /// Returns an error when any row cannot be written.
    pub fn upsert_tracks(&self, tracks: &[Track]) -> anyhow::Result<usize> {
        for track in tracks {
            self.upsert_track(track)?;
        }
        Ok(tracks.len())
    }

    /// Loads a single track by its row id.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    #[allow(dead_code)] // TODO(loki): used by a track-detail view
    pub fn get_track(&self, id: i64) -> anyhow::Result<Option<Track>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(&format!("SELECT {COLUMNS} FROM tracks WHERE id = ?1"))
            .context("failed to prepare get_track statement")?;
        let mut rows = stmt.query_map(params![id], track_from_row)?;
        rows.next().transpose().context("failed to read track")
    }

    /// Loads every track in the database.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    pub fn all_tracks(&self) -> anyhow::Result<Vec<Track>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM tracks ORDER BY title, artist"
            ))
            .context("failed to prepare all_tracks statement")?;
        let rows = stmt.query_map([], track_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to read tracks")
    }

    /// Searches tracks by case-insensitive `LIKE` on title, artist and album.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    pub fn search(&self, query: &str) -> anyhow::Result<Vec<Track>> {
        let pattern = format!("%{}%", query);
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM tracks \
                 WHERE title LIKE ?1 OR artist LIKE ?1 OR album LIKE ?1 \
                 ORDER BY title, artist"
            ))
            .context("failed to prepare search statement")?;
        let rows = stmt.query_map(params_from_iter(std::iter::once(pattern)), track_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to run search")
    }

    /// Loads every track belonging to `album`, ordered by disc and track number.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    #[allow(dead_code)] // TODO(loki): used by an album view
    pub fn tracks_by_album(&self, album: &str) -> anyhow::Result<Vec<Track>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM tracks WHERE album = ?1 \
                 ORDER BY disc_no, track_no, title"
            ))
            .context("failed to prepare tracks_by_album statement")?;
        let rows = stmt.query_map(params![album], track_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to read tracks by album")
    }

    /// Increments the `play_count` of the track with the given id.
    ///
    /// # Errors
    ///
    /// Returns an error when the track does not exist or the update fails.
    pub fn increment_play_count(&self, id: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        let changed = conn
            .execute(
                "UPDATE tracks SET play_count = play_count + 1 WHERE id = ?1",
                params![id],
            )
            .context("failed to increment play count")?;
        if changed == 0 {
            anyhow::bail!("track {id} not found");
        }
        Ok(())
    }

    /// Returns the number of tracks in the database.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    #[allow(dead_code)] // TODO(loki): shown in a library status bar
    pub fn count(&self) -> anyhow::Result<u32> {
        let conn = self.conn.lock();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0))
            .context("failed to count tracks")?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }

    // ── Playlists ───────────────────────────────────────────────────────────

    /// Creates a playlist with `name` and returns its id.
    ///
    /// # Errors
    ///
    /// Returns an error when the insert fails.
    #[allow(dead_code)] // TODO(loki): playlist management UI
    pub fn create_playlist(&self, name: &str) -> anyhow::Result<i64> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO playlists (name, created_at) VALUES (?1, ?2)",
            params![name, Utc::now().timestamp()],
        )
        .context("failed to create playlist")?;
        Ok(conn.last_insert_rowid())
    }

    /// Renames the playlist with `id`.
    ///
    /// # Errors
    ///
    /// Returns an error when the update fails.
    #[allow(dead_code)] // TODO(loki): playlist editing UI
    pub fn rename_playlist(&self, id: i64, name: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        let changed = conn
            .execute(
                "UPDATE playlists SET name = ?1 WHERE id = ?2",
                params![name, id],
            )
            .context("failed to rename playlist")?;
        if changed == 0 {
            anyhow::bail!("playlist {id} not found");
        }
        Ok(())
    }

    /// Deletes the playlist with `id` and its track references.
    ///
    /// # Errors
    ///
    /// Returns an error when the delete fails.
    #[allow(dead_code)] // TODO(loki): playlist management UI
    pub fn delete_playlist(&self, id: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])
            .context("failed to delete playlist")?;
        Ok(())
    }

    /// Lists every playlist, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    #[allow(dead_code)] // TODO(loki): playlist management UI
    pub fn list_playlists(&self) -> anyhow::Result<Vec<Playlist>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT id, name, created_at FROM playlists ORDER BY created_at DESC")
            .context("failed to prepare list_playlists statement")?;
        let rows = stmt.query_map([], |row| {
            Ok(Playlist {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: DateTime::from_timestamp(row.get(2)?, 0).unwrap_or_default(),
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to read playlists")
    }

    /// Appends `track_id` to the end of the playlist with `playlist_id`.
    ///
    /// # Errors
    ///
    /// Returns an error when either id is missing or the insert fails.
    #[allow(dead_code)] // TODO(loki): playlist editing UI
    pub fn add_to_playlist(&self, playlist_id: i64, track_id: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        let position: Option<i64> = conn
            .query_row(
                "SELECT MAX(position) FROM playlist_tracks WHERE playlist_id = ?1",
                params![playlist_id],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()
            .context("failed to read playlist size")?
            .flatten();
        conn.execute(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
            params![playlist_id, track_id, position.unwrap_or(-1) + 1],
        )
        .context("failed to add track to playlist")?;
        Ok(())
    }

    /// Removes the track at `position` from the playlist.
    ///
    /// # Errors
    ///
    /// Returns an error when the delete fails.
    #[allow(dead_code)] // TODO(loki): playlist editing UI
    pub fn remove_from_playlist(&self, playlist_id: i64, position: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND position = ?2",
            params![playlist_id, position],
        )
        .context("failed to remove track from playlist")?;
        Ok(())
    }

    /// Loads the tracks of a playlist, in playlist order.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    #[allow(dead_code)] // TODO(loki): playlist editing UI
    pub fn playlist_tracks(&self, playlist_id: i64) -> anyhow::Result<Vec<Track>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {TRACK_COLUMNS} FROM tracks
                 JOIN playlist_tracks ON tracks.id = playlist_tracks.track_id
                 WHERE playlist_tracks.playlist_id = ?1
                 ORDER BY playlist_tracks.position"
            ))
            .context("failed to prepare playlist_tracks statement")?;
        let rows = stmt.query_map(params![playlist_id], track_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to read playlist tracks")
    }

    // ── History ─────────────────────────────────────────────────────────────

    /// Records that `track_id` was played at this moment.
    ///
    /// # Errors
    ///
    /// Returns an error when the insert fails.
    pub fn record_play(&self, track_id: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO history (track_id, played_at) VALUES (?1, ?2)",
            params![track_id, Utc::now().timestamp()],
        )
        .context("failed to record play history")?;
        Ok(())
    }

    /// Returns the most recently played tracks, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    #[allow(dead_code)] // TODO(loki): history view in the GUI
    pub fn recent_history(&self, limit: u32) -> anyhow::Result<Vec<Track>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {TRACK_COLUMNS} FROM tracks
                 JOIN history ON tracks.id = history.track_id
                 ORDER BY history.played_at DESC
                 LIMIT ?1"
            ))
            .context("failed to prepare recent_history statement")?;
        let rows = stmt.query_map(params![limit], track_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to read play history")
    }

    /// Empties the play history.
    ///
    /// # Errors
    ///
    /// Returns an error when the delete fails.
    #[allow(dead_code)] // TODO(loki): history view in the GUI
    pub fn clear_history(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM history", [])
            .context("failed to clear history")?;
        Ok(())
    }
}

/// Applies incremental schema migrations for databases created by older
/// versions (drops the legacy `(source, path)` unique index and backfills the
/// `thumbnail` column).
fn migrate(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_tracks_source_path;
         PRAGMA foreign_keys = ON;",
    )
    .context("failed to run schema migration")?;
    let has_thumbnail: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('tracks') WHERE name = 'thumbnail'")
        .context("failed to inspect schema")?
        .exists([])
        .context("failed to inspect schema")?;
    if !has_thumbnail {
        conn.execute_batch("ALTER TABLE tracks ADD COLUMN thumbnail TEXT;")
            .context("failed to add thumbnail column")?;
    }
    Ok(())
}

/// Builds a [`Track`] from a single `SELECT` row.
fn track_from_row(row: &Row<'_>) -> rusqlite::Result<Track> {
    let source: String = row.get("source")?;
    let path: String = row.get("path")?;
    let last_played: Option<i64> = row.get("last_played")?;

    Ok(Track {
        id: row.get("id")?,
        source: match source.as_str() {
            "youtube" => SourceKind::Youtube,
            "soundcloud" => SourceKind::Soundcloud,
            _ => SourceKind::Local,
        },
        path: PathBuf::from(path),
        url: row.get("url")?,
        thumbnail: row.get("thumbnail")?,
        title: row.get("title")?,
        artist: row.get("artist")?,
        album: row.get("album")?,
        album_artist: row.get("album_artist")?,
        duration: Duration::from_millis(row.get::<_, i64>("duration_ms")?.max(0) as u64),
        track_no: row.get("track_no")?,
        disc_no: row.get("disc_no")?,
        genre: row.get("genre")?,
        year: row.get("year")?,
        bpm: row.get("bpm")?,
        play_count: row.get("play_count")?,
        last_played: last_played.and_then(|secs| DateTime::from_timestamp(secs, 0)),
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use tempfile::tempdir;

    use crate::library::SourceKind;

    use super::Database;
    use super::Track;

    fn sample_track(title: &str, artist: &str, album: &str) -> Track {
        Track {
            id: 0,
            source: SourceKind::Local,
            path: PathBuf::from(format!("/music/{title}.mp3")),
            url: None,
            thumbnail: None,
            title: title.to_owned(),
            artist: artist.to_owned(),
            album: album.to_owned(),
            album_artist: String::new(),
            duration: Duration::from_secs(183),
            track_no: Some(1),
            disc_no: None,
            genre: Some("Rock".to_owned()),
            year: Some(2020),
            bpm: Some(120.0),
            play_count: 0,
            last_played: None,
        }
    }

    #[test]
    fn upsert_roundtrip_keeps_id_and_updates() {
        let dir = tempdir().expect("tempdir");
        let db = Database::open(&dir.path().join("library.db")).expect("open database");

        let first = db
            .upsert_track(&sample_track("Song", "Artist", "Album"))
            .expect("first upsert");
        assert!(first.id > 0);

        let mut second = sample_track("Song", "Artist", "Album");
        second.title = "Song (remastered)".to_owned();
        second.year = Some(2021);
        let second = db.upsert_track(&second).expect("second upsert");
        assert_eq!(
            second.id, first.id,
            "re-upserting the same track must keep its id"
        );

        let stored = db.get_track(first.id).expect("get").expect("track exists");
        assert_eq!(stored.title, "Song (remastered)");
        assert_eq!(stored.year, Some(2021));
    }

    #[test]
    fn search_matches_title_artist_and_album() {
        let dir = tempdir().expect("tempdir");
        let db = Database::open(&dir.path().join("library.db")).expect("open database");
        db.upsert_tracks(&[
            sample_track("Neon Lights", "Mega Drive", "Glow"),
            sample_track("Twilight", "Vector Graphics", "Night Drive"),
        ])
        .expect("upsert");

        let by_title = db.search("neon").expect("search");
        assert_eq!(by_title.len(), 1);
        assert_eq!(by_title[0].title, "Neon Lights");

        let by_artist = db.search("vector").expect("search");
        assert_eq!(by_artist.len(), 1);

        let by_album = db.search("night").expect("search");
        assert_eq!(by_album.len(), 1);
        assert_eq!(by_album[0].title, "Twilight");

        let none = db.search("zzz").expect("search");
        assert!(none.is_empty());
    }

    #[test]
    fn tracks_by_album_filters_and_orders() {
        let dir = tempdir().expect("tempdir");
        let db = Database::open(&dir.path().join("library.db")).expect("open database");
        db.upsert_tracks(&[
            sample_track("Track One", "Artist", "Album"),
            sample_track("Track Two", "Artist", "Album"),
            sample_track("Other", "Artist", "Other"),
        ])
        .expect("upsert");

        let tracks = db.tracks_by_album("Album").expect("tracks by album");
        assert_eq!(tracks.len(), 2);
        assert!(tracks.iter().all(|t| t.album == "Album"));
    }

    #[test]
    fn increment_play_count_updates() {
        let dir = tempdir().expect("tempdir");
        let db = Database::open(&dir.path().join("library.db")).expect("open database");
        let track = db
            .upsert_track(&sample_track("Song", "Artist", "Album"))
            .expect("upsert");
        assert_eq!(track.play_count, 0);

        db.increment_play_count(track.id).expect("increment");
        db.increment_play_count(track.id).expect("increment");

        let stored = db.get_track(track.id).expect("get").expect("track exists");
        assert_eq!(stored.play_count, 2);
    }

    #[test]
    fn count_tracks() {
        let dir = tempdir().expect("tempdir");
        let db = Database::open(&dir.path().join("library.db")).expect("open database");
        assert_eq!(db.count().expect("count"), 0);

        db.upsert_tracks(&[
            sample_track("One", "Artist", "Album"),
            sample_track("Two", "Artist", "Album"),
        ])
        .expect("upsert");

        assert_eq!(db.count().expect("count"), 2);
    }

    fn remote_track(url: &str) -> Track {
        Track {
            id: 0,
            source: SourceKind::Youtube,
            path: PathBuf::new(),
            url: Some(url.to_owned()),
            thumbnail: Some("https://i.ytimg.com/vi/x/hqdefault.jpg".to_owned()),
            title: "Remote".to_owned(),
            artist: "Uploader".to_owned(),
            album: "YouTube".to_owned(),
            album_artist: String::new(),
            duration: Duration::from_secs(120),
            track_no: None,
            disc_no: None,
            genre: None,
            year: None,
            bpm: None,
            play_count: 0,
            last_played: None,
        }
    }

    #[test]
    fn remote_tracks_with_empty_path_do_not_collide() {
        let dir = tempdir().expect("tempdir");
        let db = Database::open(&dir.path().join("library.db")).expect("open database");

        let first = db
            .upsert_track(&remote_track("https://youtube.com/watch?v=aaa"))
            .expect("first remote");
        let second = db
            .upsert_track(&remote_track("https://youtube.com/watch?v=bbb"))
            .expect("second remote");
        assert_ne!(first.id, second.id, "distinct urls must get distinct rows");

        // Re-upserting the same url keeps the same row.
        let again = db
            .upsert_track(&remote_track("https://youtube.com/watch?v=aaa"))
            .expect("re-upsert");
        assert_eq!(again.id, first.id);
        assert_eq!(
            again.thumbnail.as_deref(),
            Some("https://i.ytimg.com/vi/x/hqdefault.jpg")
        );
    }

    #[test]
    fn playlists_roundtrip() {
        let dir = tempdir().expect("tempdir");
        let db = Database::open(&dir.path().join("library.db")).expect("open database");
        let track = db
            .upsert_track(&sample_track("Song", "Artist", "Album"))
            .expect("upsert");

        let playlist = db.create_playlist("Road trip").expect("create");
        db.add_to_playlist(playlist, track.id).expect("add");

        let playlists = db.list_playlists().expect("list");
        assert_eq!(playlists.len(), 1);
        assert_eq!(playlists[0].name, "Road trip");

        let tracks = db.playlist_tracks(playlist).expect("tracks");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].title, "Song");

        db.delete_playlist(playlist).expect("delete");
        assert!(db.list_playlists().expect("list").is_empty());
    }

    #[test]
    fn history_records_recent_plays() {
        let dir = tempdir().expect("tempdir");
        let db = Database::open(&dir.path().join("library.db")).expect("open database");
        let track = db
            .upsert_track(&sample_track("Song", "Artist", "Album"))
            .expect("upsert");

        db.record_play(track.id).expect("record");
        let recent = db.recent_history(10).expect("recent");
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].title, "Song");

        db.clear_history().expect("clear");
        assert!(db.recent_history(10).expect("recent").is_empty());
    }
}
