use crate::audio::AudioPlayer;
use crate::database::Database;
use crate::metadata::{write_metadata, TrackMetadata};
use crate::scanner::scan_directory;
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tauri::State;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Track {
    pub id: i64,
    pub file_path: String,
    pub folder_path: String,
    pub file_name: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub track_number: Option<i32>,
    pub duration: Option<f64>,
    pub play_count: i32,
    pub date_added: i64,
    pub last_played: Option<i64>,
    pub is_missing: bool,
    pub date_modified: Option<i64>,
    pub rating: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Tag {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub date_created: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WatchFolder {
    pub id: i64,
    pub path: String,
    pub last_scanned: Option<i64>,
    pub is_disabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Device {
    pub id: i64,
    pub path: String,
    pub name: String,
}

pub struct AppState {
    pub audio_player: Arc<Mutex<AudioPlayer>>,
    pub database: Arc<Mutex<Option<Database>>>,
}

#[tauri::command]
pub fn open_devtools(window: tauri::WebviewWindow) {
    window.open_devtools();
}

#[tauri::command]
pub async fn initialize_database(db_path: String, state: State<'_, AppState>) -> Result<(), String> {
    let db = Database::new(Path::new(&db_path)).map_err(|e: rusqlite::Error| e.to_string())?;
    let mut db_guard = (*state).database.lock().unwrap();
    *db_guard = Some(db);
    Ok(())
}

#[tauri::command]
pub async fn scan_folder(folder_path: String, state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    let path = Path::new(&folder_path);
    let scanned_tracks = scan_directory(path);

    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let mut tracks = Vec::new();
    let now = Utc::now().timestamp();

    for scanned in scanned_tracks {
        let duration = scanned.duration.map(|d| d as f64);

        // Check if track already exists
        let existing_track: Option<(i64, i32, Option<i64>, i64)> = conn
            .query_row(
                "SELECT id, play_count, last_played, date_added FROM tracks WHERE file_path = ?1",
                params![&scanned.file_path],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .ok();

        let (id, play_count, last_played, date_added) = if let Some((id, pc, lp, da)) = existing_track {
            // Update existing track, preserving play_count, last_played, and date_added
            conn.execute(
                "UPDATE tracks SET folder_path = ?1, file_name = ?2, title = ?3, artist = ?4, album = ?5, genre = ?6, year = ?7, track_number = ?8, duration = ?9, is_missing = 0, date_modified = ?10, rating = ?11 WHERE file_path = ?12",
                params![
                    scanned.folder_path,
                    scanned.file_name,
                    scanned.title,
                    scanned.artist,
                    scanned.album,
                    scanned.genre,
                    scanned.year,
                    scanned.track_number.map(|t| t as i32),
                    duration,
                    now,
                    0,
                    scanned.file_path,
                ],
            ).map_err(|e: rusqlite::Error| e.to_string())?;
            (id, pc, lp, da)
        } else {
            // Insert new track
            conn.execute(
                "INSERT INTO tracks
                 (file_path, folder_path, file_name, title, artist, album, genre, year, track_number, duration, play_count, date_added, last_played, is_missing, date_modified, rating)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    scanned.file_path,
                    scanned.folder_path,
                    scanned.file_name,
                    scanned.title,
                    scanned.artist,
                    scanned.album,
                    scanned.genre,
                    scanned.year,
                    scanned.track_number.map(|t| t as i32),
                    duration,
                    0,
                    now,
                    None::<i64>,
                    false,
                    now,
                    0,
                ],
            ).map_err(|e: rusqlite::Error| e.to_string())?;
            let id = conn.last_insert_rowid();
            (id, 0, None, now)
        };

        tracks.push(Track {
            id,
            file_path: scanned.file_path,
            folder_path: scanned.folder_path,
            file_name: scanned.file_name,
            title: scanned.title,
            artist: scanned.artist,
            album: scanned.album,
            genre: scanned.genre,
            year: scanned.year,
            track_number: scanned.track_number.map(|t| t as i32),
            duration,
            play_count,
            date_added,
            last_played,
            is_missing: false,
            date_modified: Some(now),
            rating: 0,
        });
    }

    Ok(tracks)
}

#[tauri::command]
pub async fn get_all_tracks(state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let mut stmt = conn
        .prepare("SELECT id, file_path, folder_path, file_name, title, artist, album, genre, year, track_number, duration, play_count, date_added, last_played, is_missing, date_modified, rating FROM tracks WHERE is_missing = 0")
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let tracks = stmt
        .query_map([], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                folder_path: row.get(2)?,
                file_name: row.get(3)?,
                title: row.get(4)?,
                artist: row.get(5)?,
                album: row.get(6)?,
                genre: row.get(7)?,
                year: row.get(8)?,
                track_number: row.get(9)?,
                duration: row.get(10)?,
                play_count: row.get(11)?,
                date_added: row.get(12)?,
                last_played: row.get(13)?,
                is_missing: row.get(14)?,
                date_modified: row.get(15)?,
                rating: row.get(16)?,
            })
        })
        .map_err(|e: rusqlite::Error| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(tracks)
}

#[tauri::command]
pub async fn search_tracks(query: String, state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let search_pattern = format!("%{}%", query);
    
    let mut stmt = conn
        .prepare(
            "SELECT id, file_path, folder_path, file_name, title, artist, album, genre, year, track_number, duration, play_count, date_added, last_played, is_missing, date_modified, rating 
             FROM tracks 
             WHERE is_missing = 0 
             AND (title LIKE ?1 OR artist LIKE ?1 OR album LIKE ?1 OR genre LIKE ?1 OR file_name LIKE ?1 OR folder_path LIKE ?1)"
        )
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let tracks = stmt
        .query_map(params![search_pattern], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                folder_path: row.get(2)?,
                file_name: row.get(3)?,
                title: row.get(4)?,
                artist: row.get(5)?,
                album: row.get(6)?,
                genre: row.get(7)?,
                year: row.get(8)?,
                track_number: row.get(9)?,
                duration: row.get(10)?,
                play_count: row.get(11)?,
                date_added: row.get(12)?,
                last_played: row.get(13)?,
                is_missing: row.get(14)?,
                date_modified: row.get(15)?,
                rating: row.get(16)?,
            })
        })
        .map_err(|e: rusqlite::Error| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(tracks)
}

#[tauri::command]
pub async fn play_track(file_path: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = Path::new(&file_path);
    let mut player = (*state).audio_player.lock().unwrap();
    player.play_file(path).map_err(|e: Box<dyn std::error::Error>| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn pause_playback(state: State<'_, AppState>) -> Result<(), String> {
    let player = (*state).audio_player.lock().unwrap();
    player.pause();
    Ok(())
}

#[tauri::command]
pub async fn resume_playback(state: State<'_, AppState>) -> Result<(), String> {
    let player = (*state).audio_player.lock().unwrap();
    player.resume();
    Ok(())
}

#[tauri::command]
pub async fn stop_playback(state: State<'_, AppState>) -> Result<(), String> {
    let mut player = (*state).audio_player.lock().unwrap();
    player.stop();
    Ok(())
}

#[tauri::command]
pub async fn set_volume(volume: f64, state: State<'_, AppState>) -> Result<(), String> {
    let player = (*state).audio_player.lock().unwrap();
    player.set_volume(volume as f32);
    Ok(())
}

#[tauri::command]
pub async fn seek(position: f64, state: State<'_, AppState>) -> Result<(), String> {
    let player = (*state).audio_player.lock().unwrap();
    player.seek(position);
    Ok(())
}

#[tauri::command]
pub async fn get_frequency_data(state: State<'_, AppState>) -> Result<Vec<u8>, String> {
    let player = (*state).audio_player.lock().unwrap();
    Ok(player.get_frequency_data())
}

#[tauri::command]
pub async fn update_frequency_data(is_playing: bool, volume: f64, state: State<'_, AppState>) -> Result<(), String> {
    let player = (*state).audio_player.lock().unwrap();
    player.update_frequency_data(is_playing, volume as f32);
    Ok(())
}

#[tauri::command]
pub async fn is_playing(state: State<'_, AppState>) -> Result<bool, String> {
    let player = (*state).audio_player.lock().unwrap();
    Ok(player.is_playing())
}

#[tauri::command]
pub async fn update_track_metadata(
    file_path: String,
    metadata: TrackMetadata,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = Path::new(&file_path);
    write_metadata(path, &metadata).map_err(|e: Box<dyn std::error::Error>| e.to_string())?;

    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    conn.execute(
        "UPDATE tracks SET title = ?1, artist = ?2, album = ?3, genre = ?4, year = ?5, track_number = ?6 WHERE file_path = ?7",
        params![
            metadata.title,
            metadata.artist,
            metadata.album,
            metadata.genre,
            metadata.year,
            metadata.track_number.map(|t| t as i32),
            file_path,
        ],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn increment_play_count(file_path: String, state: State<'_, AppState>) -> Result<(), String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let now = Utc::now().timestamp();
    
    conn.execute(
        "UPDATE tracks SET play_count = play_count + 1, last_played = ?1 WHERE file_path = ?2",
        params![now, file_path],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn add_tag(name: String, state: State<'_, AppState>) -> Result<Tag, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    conn.execute(
        "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
        params![name],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    let id = conn.last_insert_rowid();
    
    Ok(Tag { id, name })
}

#[tauri::command]
pub async fn get_all_tags(state: State<'_, AppState>) -> Result<Vec<Tag>, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let mut stmt = conn
        .prepare("SELECT id, name FROM tags")
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let tags = stmt
        .query_map([], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })
        .map_err(|e: rusqlite::Error| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(tags)
}

#[tauri::command]
pub async fn add_tag_to_track(track_id: i64, tag_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    conn.execute(
        "INSERT OR IGNORE INTO track_tags (track_id, tag_id) VALUES (?1, ?2)",
        params![track_id, tag_id],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn remove_tag_from_track(track_id: i64, tag_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    conn.execute(
        "DELETE FROM track_tags WHERE track_id = ?1 AND tag_id = ?2",
        params![track_id, tag_id],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn get_tracks_by_tag(tag_id: i64, state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.file_path, t.folder_path, t.file_name, t.title, t.artist, t.album, t.genre, t.year, t.track_number, t.duration, t.play_count, t.date_added, t.last_played, t.is_missing, t.date_modified, t.rating 
             FROM tracks t 
             INNER JOIN track_tags tt ON t.id = tt.track_id 
             WHERE tt.tag_id = ?1 AND t.is_missing = 0"
        )
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let tracks = stmt
        .query_map(params![tag_id], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                folder_path: row.get(2)?,
                file_name: row.get(3)?,
                title: row.get(4)?,
                artist: row.get(5)?,
                album: row.get(6)?,
                genre: row.get(7)?,
                year: row.get(8)?,
                track_number: row.get(9)?,
                duration: row.get(10)?,
                play_count: row.get(11)?,
                date_added: row.get(12)?,
                last_played: row.get(13)?,
                is_missing: row.get(14)?,
                date_modified: row.get(15)?,
                rating: row.get(16)?,
            })
        })
        .map_err(|e: rusqlite::Error| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(tracks)
}

#[tauri::command]
pub async fn create_playlist(name: String, state: State<'_, AppState>) -> Result<Playlist, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let now = Utc::now().timestamp();
    
    conn.execute(
        "INSERT INTO playlists (name, date_created) VALUES (?1, ?2)",
        params![name, now],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    let id = conn.last_insert_rowid();
    
    Ok(Playlist { id, name, date_created: now })
}

#[tauri::command]
pub async fn get_all_playlists(state: State<'_, AppState>) -> Result<Vec<Playlist>, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let mut stmt = conn
        .prepare("SELECT id, name, date_created FROM playlists")
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let playlists = stmt
        .query_map([], |row| {
            Ok(Playlist {
                id: row.get(0)?,
                name: row.get(1)?,
                date_created: row.get(2)?,
            })
        })
        .map_err(|e: rusqlite::Error| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(playlists)
}

#[tauri::command]
pub async fn add_track_to_playlist(playlist_id: i64, track_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    // Get the current max position
    let position: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM playlist_tracks WHERE playlist_id = ?1",
            params![playlist_id],
            |row| row.get(0),
        )
        .map_err(|e: rusqlite::Error| e.to_string())?;

    conn.execute(
        "INSERT OR IGNORE INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
        params![playlist_id, track_id, position],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn is_track_in_playlist(playlist_id: i64, track_id: i64, state: State<'_, AppState>) -> Result<bool, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let count: i32 = conn
        .query_row(
            "SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id = ?1 AND track_id = ?2",
            params![playlist_id, track_id],
            |row| row.get(0),
        )
        .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(count > 0)
}

#[tauri::command]
pub async fn get_playlist_track_count(playlist_id: i64, state: State<'_, AppState>) -> Result<i32, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let count: i32 = conn
        .query_row(
            "SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id = ?1",
            params![playlist_id],
            |row| row.get(0),
        )
        .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(count)
}

#[tauri::command]
pub async fn delete_playlist(playlist_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    // Delete playlist tracks first (foreign key constraint)
    conn.execute(
        "DELETE FROM playlist_tracks WHERE playlist_id = ?1",
        params![playlist_id],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    // Delete the playlist
    conn.execute(
        "DELETE FROM playlists WHERE id = ?1",
        params![playlist_id],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn reveal_in_file_explorer(file_path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        Command::new("explorer")
            .args(["/select,", &file_path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        Command::new("open")
            .args(["-R", &file_path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        // Try to use the file manager to reveal the file
        // This varies by desktop environment, so we'll try a few common approaches
        let result = Command::new("dbus-send")
            .args([
                "--session",
                "--dest=org.freedesktop.FileManager1",
                "--type=method_call",
                "/org/freedesktop/FileManager1",
                "org.freedesktop.FileManager1.ShowItems",
                "array:string:file://".to_string() + &file_path,
                "string:",
            ])
            .spawn();

        if result.is_ok() {
            return Ok(());
        }

        // Fallback: try to open the directory
        if let Some(parent) = std::path::Path::new(&file_path).parent() {
            Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn get_playlist_tracks(playlist_id: i64, state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.file_path, t.folder_path, t.file_name, t.title, t.artist, t.album, t.genre, t.year, t.track_number, t.duration, t.play_count, t.date_added, t.last_played, t.is_missing, t.date_modified, t.rating 
             FROM tracks t 
             INNER JOIN playlist_tracks pt ON t.id = pt.track_id 
             WHERE pt.playlist_id = ?1 AND t.is_missing = 0 
             ORDER BY pt.position"
        )
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let tracks = stmt
        .query_map(params![playlist_id], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                folder_path: row.get(2)?,
                file_name: row.get(3)?,
                title: row.get(4)?,
                artist: row.get(5)?,
                album: row.get(6)?,
                genre: row.get(7)?,
                year: row.get(8)?,
                track_number: row.get(9)?,
                duration: row.get(10)?,
                play_count: row.get(11)?,
                date_added: row.get(12)?,
                last_played: row.get(13)?,
                is_missing: row.get(14)?,
                date_modified: row.get(15)?,
                rating: row.get(16)?,
            })
        })
        .map_err(|e: rusqlite::Error| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(tracks)
}

#[tauri::command]
pub async fn remove_track_from_playlist(playlist_id: i64, track_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    conn.execute(
        "DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND track_id = ?2",
        params![playlist_id, track_id],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn add_watch_folder(path: String, state: State<'_, AppState>) -> Result<WatchFolder, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    conn.execute(
        "INSERT OR IGNORE INTO watch_folders (path, last_scanned, is_disabled) VALUES (?1, NULL, 0)",
        params![path],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    let id = conn.last_insert_rowid();
    
    Ok(WatchFolder { id, path, last_scanned: None, is_disabled: false })
}

#[tauri::command]
pub async fn get_watch_folders(state: State<'_, AppState>) -> Result<Vec<WatchFolder>, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let mut stmt = conn
        .prepare("SELECT id, path, last_scanned, is_disabled FROM watch_folders")
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let folders = stmt
        .query_map([], |row| {
            Ok(WatchFolder {
                id: row.get(0)?,
                path: row.get(1)?,
                last_scanned: row.get(2)?,
                is_disabled: row.get(3)?,
            })
        })
        .map_err(|e: rusqlite::Error| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(folders)
}

#[tauri::command]
pub async fn remove_watch_folder(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    conn.execute(
        "DELETE FROM watch_folders WHERE id = ?1",
        params![id],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn toggle_watch_folder(id: i64, is_disabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    conn.execute(
        "UPDATE watch_folders SET is_disabled = ?1 WHERE id = ?2",
        params![is_disabled, id],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn mark_missing_tracks(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let mut stmt = conn
        .prepare("SELECT file_path FROM tracks WHERE is_missing = 0")
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let file_paths: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e: rusqlite::Error| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let mut missing_files = Vec::new();

    for file_path in file_paths {
        if !Path::new(&file_path).exists() {
            conn.execute(
                "UPDATE tracks SET is_missing = 1 WHERE file_path = ?1",
                params![file_path],
            ).map_err(|e: rusqlite::Error| e.to_string())?;
            missing_files.push(file_path);
        }
    }

    Ok(missing_files)
}

#[tauri::command]
pub async fn remove_missing_tracks(state: State<'_, AppState>) -> Result<(), String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    conn.execute(
        "DELETE FROM tracks WHERE is_missing = 1",
        [],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn get_recently_added(limit: i32, state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let mut stmt = conn
        .prepare(
            "SELECT id, file_path, folder_path, file_name, title, artist, album, genre, year, track_number, duration, play_count, date_added, last_played, is_missing, date_modified, rating 
             FROM tracks 
             WHERE is_missing = 0 
             ORDER BY date_added DESC 
             LIMIT ?1"
        )
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let tracks = stmt
        .query_map(params![limit], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                folder_path: row.get(2)?,
                file_name: row.get(3)?,
                title: row.get(4)?,
                artist: row.get(5)?,
                album: row.get(6)?,
                genre: row.get(7)?,
                year: row.get(8)?,
                track_number: row.get(9)?,
                duration: row.get(10)?,
                play_count: row.get(11)?,
                date_added: row.get(12)?,
                last_played: row.get(13)?,
                is_missing: row.get(14)?,
                date_modified: row.get(15)?,
                rating: row.get(16)?,
            })
        })
        .map_err(|e: rusqlite::Error| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(tracks)
}

#[tauri::command]
pub async fn get_never_played(state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let mut stmt = conn
        .prepare(
            "SELECT id, file_path, folder_path, file_name, title, artist, album, genre, year, track_number, duration, play_count, date_added, last_played, is_missing, date_modified, rating
             FROM tracks
             WHERE is_missing = 0 AND play_count = 0"
        )
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let tracks = stmt
        .query_map([], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                folder_path: row.get(2)?,
                file_name: row.get(3)?,
                title: row.get(4)?,
                artist: row.get(5)?,
                album: row.get(6)?,
                genre: row.get(7)?,
                year: row.get(8)?,
                track_number: row.get(9)?,
                duration: row.get(10)?,
                play_count: row.get(11)?,
                date_added: row.get(12)?,
                last_played: row.get(13)?,
                is_missing: row.get(14)?,
                date_modified: row.get(15)?,
                rating: row.get(16)?,
            })
        })
        .map_err(|e: rusqlite::Error| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(tracks)
}

#[tauri::command]
pub async fn get_tracks_not_in_playlist(state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let mut stmt = conn
        .prepare(
            "SELECT id, file_path, folder_path, file_name, title, artist, album, genre, year, track_number, duration, play_count, date_added, last_played, is_missing, date_modified, rating
             FROM tracks
             WHERE is_missing = 0 AND id NOT IN (SELECT track_id FROM playlist_tracks)"
        )
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let tracks = stmt
        .query_map([], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                folder_path: row.get(2)?,
                file_name: row.get(3)?,
                title: row.get(4)?,
                artist: row.get(5)?,
                album: row.get(6)?,
                genre: row.get(7)?,
                year: row.get(8)?,
                track_number: row.get(9)?,
                duration: row.get(10)?,
                play_count: row.get(11)?,
                date_added: row.get(12)?,
                last_played: row.get(13)?,
                is_missing: row.get(14)?,
                date_modified: row.get(15)?,
                rating: row.get(16)?,
            })
        })
        .map_err(|e: rusqlite::Error| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(tracks)
}

#[tauri::command]
pub async fn get_most_played(limit: i32, state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let mut stmt = conn
        .prepare(
            "SELECT id, file_path, folder_path, file_name, title, artist, album, genre, year, track_number, duration, play_count, date_added, last_played, is_missing, date_modified, rating 
             FROM tracks 
             WHERE is_missing = 0 
             ORDER BY play_count DESC 
             LIMIT ?1"
        )
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let tracks = stmt
        .query_map(params![limit], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                folder_path: row.get(2)?,
                file_name: row.get(3)?,
                title: row.get(4)?,
                artist: row.get(5)?,
                album: row.get(6)?,
                genre: row.get(7)?,
                year: row.get(8)?,
                track_number: row.get(9)?,
                duration: row.get(10)?,
                play_count: row.get(11)?,
                date_added: row.get(12)?,
                last_played: row.get(13)?,
                is_missing: row.get(14)?,
                date_modified: row.get(15)?,
                rating: row.get(16)?,
            })
        })
        .map_err(|e: rusqlite::Error| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(tracks)
}

#[tauri::command]
pub async fn get_recently_played(limit: i32, state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let mut stmt = conn
        .prepare(
            "SELECT id, file_path, folder_path, file_name, title, artist, album, genre, year, track_number, duration, play_count, date_added, last_played, is_missing, date_modified, rating 
             FROM tracks 
             WHERE is_missing = 0 AND last_played IS NOT NULL 
             ORDER BY last_played DESC 
             LIMIT ?1"
        )
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let tracks = stmt
        .query_map(params![limit], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                folder_path: row.get(2)?,
                file_name: row.get(3)?,
                title: row.get(4)?,
                artist: row.get(5)?,
                album: row.get(6)?,
                genre: row.get(7)?,
                year: row.get(8)?,
                track_number: row.get(9)?,
                duration: row.get(10)?,
                play_count: row.get(11)?,
                date_added: row.get(12)?,
                last_played: row.get(13)?,
                is_missing: row.get(14)?,
                date_modified: row.get(15)?,
                rating: row.get(16)?,
            })
        })
        .map_err(|e: rusqlite::Error| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(tracks)
}

#[tauri::command]
pub async fn get_recently_modified(limit: i32, state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let mut stmt = conn
        .prepare(
            "SELECT id, file_path, folder_path, file_name, title, artist, album, genre, year, track_number, duration, play_count, date_added, last_played, is_missing, date_modified, rating 
             FROM tracks 
             WHERE is_missing = 0 AND date_modified IS NOT NULL 
             ORDER BY date_modified DESC 
             LIMIT ?1"
        )
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let tracks = stmt
        .query_map(params![limit], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                folder_path: row.get(2)?,
                file_name: row.get(3)?,
                title: row.get(4)?,
                artist: row.get(5)?,
                album: row.get(6)?,
                genre: row.get(7)?,
                year: row.get(8)?,
                track_number: row.get(9)?,
                duration: row.get(10)?,
                play_count: row.get(11)?,
                date_added: row.get(12)?,
                last_played: row.get(13)?,
                is_missing: row.get(14)?,
                date_modified: row.get(15)?,
                rating: row.get(16)?,
            })
        })
        .map_err(|e: rusqlite::Error| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(tracks)
}

#[tauri::command]
pub async fn get_top_rated(limit: i32, state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let mut stmt = conn
        .prepare(
            "SELECT id, file_path, folder_path, file_name, title, artist, album, genre, year, track_number, duration, play_count, date_added, last_played, is_missing, date_modified, rating 
             FROM tracks 
             WHERE is_missing = 0 AND rating > 0 
             ORDER BY rating DESC 
             LIMIT ?1"
        )
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let tracks = stmt
        .query_map(params![limit], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                folder_path: row.get(2)?,
                file_name: row.get(3)?,
                title: row.get(4)?,
                artist: row.get(5)?,
                album: row.get(6)?,
                genre: row.get(7)?,
                year: row.get(8)?,
                track_number: row.get(9)?,
                duration: row.get(10)?,
                play_count: row.get(11)?,
                date_added: row.get(12)?,
                last_played: row.get(13)?,
                is_missing: row.get(14)?,
                date_modified: row.get(15)?,
                rating: row.get(16)?,
            })
        })
        .map_err(|e: rusqlite::Error| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(tracks)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Drive {
    pub letter: String,
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Folder {
    pub path: String,
    pub name: String,
}

#[tauri::command]
pub async fn get_drives() -> Result<Vec<Drive>, String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let output = Command::new("powershell")
            .args([
                "-Command",
                "Get-Volume | Where-Object { $_.DriveLetter } | Select-Object DriveLetter, FileSystemLabel | ConvertTo-Json"
            ])
            .output()
            .map_err(|e| e.to_string())?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut drives = Vec::new();

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&output_str) {
            if let Some(arr) = json.as_array() {
                for item in arr {
                    if let Some(drive_letter) = item.get("DriveLetter").and_then(|v| v.as_str()) {
                        let letter = format!("{}:\\", drive_letter);
                        let name = item.get("FileSystemLabel")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string());
                        drives.push(Drive { letter, name });
                    }
                }
            }
        }

        // Fallback: iterate through drive letters if PowerShell failed
        if drives.is_empty() {
            for letter in b'A'..=b'Z' {
                let drive_path = format!("{}:\\", letter as char);
                let path = Path::new(&drive_path);

                if path.exists() {
                    drives.push(Drive {
                        letter: drive_path,
                        name: None,
                    });
                }
            }
        }

        Ok(drives)
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let output = Command::new("df")
            .args(["-H"])
            .output()
            .map_err(|e| e.to_string())?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut drives = Vec::new();

        for line in output_str.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 1 {
                let path = parts[0].to_string();
                drives.push(Drive {
                    letter: path.clone(),
                    name: Some(path),
                });
            }
        }

        Ok(drives)
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        let output = Command::new("df")
            .args(["-H"])
            .output()
            .map_err(|e| e.to_string())?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut drives = Vec::new();

        for line in output_str.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 1 {
                let path = parts[0].to_string();
                drives.push(Drive {
                    letter: path.clone(),
                    name: Some(path),
                });
            }
        }

        Ok(drives)
    }
}

#[tauri::command]
pub async fn get_drive_folders(drive_path: String) -> Result<Vec<Folder>, String> {
    let path = Path::new(&drive_path);
    let mut folders = Vec::new();

    if path.exists() && path.is_dir() {
        let entries = std::fs::read_dir(path).map_err(|e| e.to_string())?;

        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let entry_path = entry.path();

            if entry_path.is_dir() {
                let name = entry_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let full_path = entry_path.to_string_lossy().to_string();
                folders.push(Folder {
                    path: full_path,
                    name,
                });
            }
        }
    }

    folders.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(folders)
}

#[tauri::command]
pub async fn add_device(path: String, name: String, state: State<'_, AppState>) -> Result<Device, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    conn.execute(
        "INSERT OR IGNORE INTO devices (path, name) VALUES (?1, ?2)",
        params![path, name],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    let id = conn.last_insert_rowid();

    Ok(Device { id, path, name })
}

#[tauri::command]
pub async fn remove_device(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    conn.execute(
        "DELETE FROM devices WHERE id = ?1",
        params![id],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn get_devices(state: State<'_, AppState>) -> Result<Vec<Device>, String> {
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let mut stmt = conn
        .prepare("SELECT id, path, name FROM devices")
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let devices = stmt
        .query_map([], |row| {
            Ok(Device {
                id: row.get(0)?,
                path: row.get(1)?,
                name: row.get(2)?,
            })
        })
        .map_err(|e: rusqlite::Error| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(devices)
}

#[tauri::command]
pub async fn check_device_reachable(path: String) -> Result<bool, String> {
    Ok(Path::new(&path).exists())
}

#[tauri::command]
pub async fn get_device_tracks(device_path: String, state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    let path = Path::new(&device_path);

    if !path.exists() {
        return Err("Device path does not exist".to_string());
    }

    let scanned_tracks = scan_directory(path);

    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();

    let mut tracks = Vec::new();
    let mut next_temp_id = -1; // Use negative IDs for device tracks not in database

    for scanned in scanned_tracks {
        let duration = scanned.duration.map(|d| d as f64);

        // Check if track already exists in database
        let existing_track: Option<(i64, i32, Option<i64>, i64, i32, Option<i64>)> = conn
            .query_row(
                "SELECT id, play_count, last_played, date_added, rating, date_modified FROM tracks WHERE file_path = ?1",
                params![&scanned.file_path],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            )
            .ok();

        let (id, play_count, last_played, date_added, rating, date_modified) = if let Some((id, pc, lp, da, r, dm)) = existing_track {
            (id, pc, lp, da, r, dm)
        } else {
            // For device tracks not in database, use a unique negative ID
            let temp_id = next_temp_id;
            next_temp_id -= 1;
            let now = Utc::now().timestamp();
            (temp_id, 0, None, now, 0, Some(now))
        };

        tracks.push(Track {
            id,
            file_path: scanned.file_path,
            folder_path: scanned.folder_path,
            file_name: scanned.file_name,
            title: scanned.title,
            artist: scanned.artist,
            album: scanned.album,
            genre: scanned.genre,
            year: scanned.year,
            track_number: scanned.track_number.map(|t| t as i32),
            duration,
            play_count,
            date_added,
            last_played,
            is_missing: false,
            date_modified,
            rating,
        });
    }

    Ok(tracks)
}

#[tauri::command]
pub async fn copy_file_to_device(source_path: String, device_path: String) -> Result<String, String> {
    let source = Path::new(&source_path);
    let device = Path::new(&device_path);

    if !source.exists() {
        return Err("Source file does not exist".to_string());
    }

    if !device.exists() {
        return Err("Device path does not exist".to_string());
    }

    // Get the file name from source
    let file_name = source.file_name()
        .and_then(|n| n.to_str())
        .ok_or("Failed to get file name")?;

    // Create destination path
    let destination = device.join(file_name);

    // Check if file already exists at destination
    if destination.exists() {
        return Err("File already exists at destination".to_string());
    }

    // Copy the file
    std::fs::copy(source, &destination)
        .map_err(|e| format!("Failed to copy file: {}", e))?;

    Ok(destination.to_string_lossy().to_string())
}
