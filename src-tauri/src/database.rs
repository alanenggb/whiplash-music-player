use rusqlite::{Connection, Result};
use std::path::Path;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let db = Database { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        // Tracks table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS tracks (
                id INTEGER PRIMARY KEY,
                file_path TEXT UNIQUE NOT NULL,
                folder_path TEXT NOT NULL,
                file_name TEXT NOT NULL,
                title TEXT,
                artist TEXT,
                album TEXT,
                genre TEXT,
                year INTEGER,
                track_number INTEGER,
                duration REAL,
                play_count INTEGER DEFAULT 0,
                date_added INTEGER NOT NULL,
                last_played INTEGER,
                is_missing BOOLEAN DEFAULT 0,
                date_modified INTEGER,
                rating INTEGER DEFAULT 0
            )",
            [],
        )?;

        // Tags table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS tags (
                id INTEGER PRIMARY KEY,
                name TEXT UNIQUE NOT NULL
            )",
            [],
        )?;

        // Track-Tag junction table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS track_tags (
                track_id INTEGER NOT NULL,
                tag_id INTEGER NOT NULL,
                PRIMARY KEY (track_id, tag_id),
                FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE,
                FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Playlists table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS playlists (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                date_created INTEGER NOT NULL
            )",
            [],
        )?;

        // Playlist-Track junction table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS playlist_tracks (
                playlist_id INTEGER NOT NULL,
                track_id INTEGER NOT NULL,
                position INTEGER NOT NULL,
                PRIMARY KEY (playlist_id, track_id),
                FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE,
                FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Watch folders table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS watch_folders (
                id INTEGER PRIMARY KEY,
                path TEXT UNIQUE NOT NULL,
                last_scanned INTEGER,
                is_disabled BOOLEAN DEFAULT 0
            )",
            [],
        )?;

        // Devices table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS devices (
                id INTEGER PRIMARY KEY,
                path TEXT UNIQUE NOT NULL,
                name TEXT NOT NULL
            )",
            [],
        )?;

        // Attempt to add is_disabled column to existing databases
        // This is safe even if the column already exists (the statement will fail and we ignore it)
        let _ = self.conn.execute(
            "ALTER TABLE watch_folders ADD COLUMN is_disabled BOOLEAN DEFAULT 0",
            [],
        );

        // Attempt to add new columns to tracks table for existing databases
        let _ = self.conn.execute(
            "ALTER TABLE tracks ADD COLUMN date_modified INTEGER",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE tracks ADD COLUMN rating INTEGER DEFAULT 0",
            [],
        );

        // App settings table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;

        // Create indexes
        self.create_indexes()?;

        Ok(())
    }

    fn create_indexes(&self) -> Result<()> {
        let indexes = vec![
            "CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist)",
            "CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album)",
            "CREATE INDEX IF NOT EXISTS idx_tracks_genre ON tracks(genre)",
            "CREATE INDEX IF NOT EXISTS idx_tracks_year ON tracks(year)",
            "CREATE INDEX IF NOT EXISTS idx_tracks_date_added ON tracks(date_added)",
            "CREATE INDEX IF NOT EXISTS idx_tracks_play_count ON tracks(play_count)",
            "CREATE INDEX IF NOT EXISTS idx_tracks_folder ON tracks(folder_path)",
            "CREATE INDEX IF NOT EXISTS idx_tracks_file_name ON tracks(file_name)",
            "CREATE INDEX IF NOT EXISTS idx_track_tags_tag_id ON track_tags(tag_id)",
        ];

        for index in indexes {
            self.conn.execute(index, [])?;
        }

        Ok(())
    }

    pub fn get_connection(&self) -> &Connection {
        &self.conn
    }
}
