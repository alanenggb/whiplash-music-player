use crate::audio::AudioPlayer;
use crate::database::Database;
use crate::metadata::{write_metadata, TrackMetadata};
use crate::scanner::{scan_directory, scan_single_file};
use chrono::Utc;
use dirs;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tauri::State;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Serialize, Deserialize, Clone)]
struct SpotifyToken {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    expires_at: i64, // Unix timestamp
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SpotifyPlaylist {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub external_urls: serde_json::Value,
}

fn get_cache_path() -> Result<std::path::PathBuf, String> {
    let data_dir = dirs::data_dir().ok_or("Failed to get data directory")?;
    let cache_path = data_dir.join("whiplash").join("spotify_token.json");
    Ok(cache_path)
}

fn load_token() -> Option<SpotifyToken> {
    let cache_path = get_cache_path().ok()?;
    let content = std::fs::read_to_string(&cache_path).ok()?;
    serde_json::from_str(&content).ok()
}

fn save_token(token: &SpotifyToken) -> Result<(), String> {
    let cache_path = get_cache_path()?;
    std::fs::create_dir_all(cache_path.parent().unwrap())
        .map_err(|e| format!("Failed to create cache directory: {}", e))?;
    std::fs::write(&cache_path, serde_json::to_string_pretty(token).unwrap())
        .map_err(|e| format!("Failed to save token: {}", e))?;
    Ok(())
}

fn is_token_valid(token: &SpotifyToken) -> bool {
    let now = Utc::now().timestamp();
    // Consider token valid if it has more than 60 seconds remaining
    now < token.expires_at - 60
}

async fn refresh_token(refresh_token: &str) -> Result<SpotifyToken, String> {
    let client_id = "a42d7e4861544e72b357a5350aebfe8d";
    let client = reqwest::Client::new();
    
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
    ];
    
    let response: reqwest::Response = client
        .post("https://accounts.spotify.com/api/token")
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Failed to refresh token: {}", e))?;
    
    if response.status().is_success() {
        let token_response: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse token response: {}", e))?;
        
        let access_token = token_response["access_token"]
            .as_str()
            .ok_or("Missing access_token")?
            .to_string();
        
        let refresh_token = token_response["refresh_token"]
            .as_str()
            .ok_or("Missing refresh_token")?
            .to_string();
        
        let expires_in = token_response["expires_in"]
            .as_i64()
            .ok_or("Missing expires_in")?;
        
        let expires_at = Utc::now().timestamp() + expires_in;
        
        Ok(SpotifyToken {
            access_token,
            refresh_token,
            expires_in,
            expires_at,
        })
    } else {
        let error_text: String = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        Err(format!("Failed to refresh token: {}", error_text))
    }
}

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
pub async fn authenticate_spotify() -> Result<String, String> {
    // Manual PKCE implementation
    use rand::Rng;
    use rand::rngs::OsRng;
    use sha2::{Digest, Sha256};
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    
    let cache_path = get_cache_path()?;
    std::fs::create_dir_all(cache_path.parent().unwrap())
        .map_err(|e| format!("Failed to create cache directory: {}", e))?;
    
    // Generate code verifier
    let code_verifier: String = {
        let mut rng = OsRng;
        (0..128)
            .map(|_| {
                let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
                chars[rng.gen_range(0..chars.len())] as char
            })
            .collect()
    };
    
    // Generate code challenge
    let code_challenge = {
        let mut hasher = Sha256::new();
        hasher.update(code_verifier.as_bytes());
        let hash = hasher.finalize();
        URL_SAFE_NO_PAD.encode(&hash)
    };
    
    let client_id = "a42d7e4861544e72b357a5350aebfe8d";
    let redirect_uri = "http://127.0.0.1:13337";
    let scopes = "playlist-read-private playlist-read-collaborative user-library-read user-read-private user-read-email";
    
    // Construct authorization URL
    let url = format!(
        "https://accounts.spotify.com/authorize?client_id={}&response_type=code&redirect_uri={}&scope={}&code_challenge_method=S256&code_challenge={}",
        client_id,
        redirect_uri,
        scopes,
        code_challenge
    );
    
    eprintln!("Authorization URL: {}", url);
    
    // Open browser
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    
    // Start server to capture callback
    let listener = tokio::net::TcpListener::bind("127.0.0.1:13337")
        .await
        .map_err(|e| format!("Failed to bind to port 13337: {}", e))?;
    
    match listener.accept().await {
        Ok((mut socket, _)) => {
            let mut buf = [0; 2048];
            let n = socket.read(&mut buf).await.map_err(|e| format!("Failed to read from socket: {}", e))?;
            let request = String::from_utf8_lossy(&buf[..n]);
            
            eprintln!("Received request: {}", request);
            
            // Extract the code from the request - stop at space or &
            let code = request
                .split("code=")
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
                .and_then(|s| s.split('&').next())
                .ok_or("Failed to extract code from callback")?;
            
            eprintln!("Extracted code: {}", code);
            
            let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body>Authentication successful! You can close this window.</body></html>";
            socket.write_all(response.as_bytes()).await.map_err(|e| format!("Failed to write response: {}", e))?;
            
            // Exchange code for token using reqwest
            let client = reqwest::Client::new();
            let params = [
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", redirect_uri),
                ("client_id", client_id),
                ("code_verifier", &code_verifier),
            ];
            
            let response: reqwest::Response = client
                .post("https://accounts.spotify.com/api/token")
                .form(&params)
                .send()
                .await
                .map_err(|e| format!("Failed to exchange code for token: {}", e))?;
            
            if response.status().is_success() {
                let token_response: serde_json::Value = response
                    .json()
                    .await
                    .map_err(|e| format!("Failed to parse token response: {}", e))?;
                
                eprintln!("Token response: {:?}", token_response);
                
                let access_token = token_response["access_token"]
                    .as_str()
                    .ok_or("Missing access_token")?
                    .to_string();
                
                let refresh_token = token_response["refresh_token"]
                    .as_str()
                    .ok_or("Missing refresh_token")?
                    .to_string();
                
                let expires_in = token_response["expires_in"]
                    .as_i64()
                    .ok_or("Missing expires_in")?;
                
                let expires_at = Utc::now().timestamp() + expires_in;
                
                let token = SpotifyToken {
                    access_token,
                    refresh_token,
                    expires_in,
                    expires_at,
                };
                
                save_token(&token)?;
                
                Ok("Autenticado com sucesso! Token salvo no cache.".to_string())
            } else {
                let error_text: String = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                eprintln!("Token exchange error: {}", error_text);
                Err(format!("Failed to get token: {}", error_text))
            }
        }
        Err(e) => Err(format!("Failed to accept connection: {}", e))
    }
}

#[tauri::command]
pub async fn get_spotify_token() -> Result<String, String> {
    // Try to load existing token
    if let Some(token) = load_token() {
        if is_token_valid(&token) {
            return Ok(token.access_token);
        }
        
        // Token expired, try to refresh
        eprintln!("Token expired, attempting refresh...");
        match refresh_token(&token.refresh_token).await {
            Ok(new_token) => {
                save_token(&new_token)?;
                eprintln!("Token refreshed successfully");
                return Ok(new_token.access_token);
            }
            Err(e) => {
                eprintln!("Failed to refresh token: {}", e);
                return Err(format!("Token expired and refresh failed: {}. Please authenticate again.", e));
            }
        }
    }
    
    Err("No valid token found. Please authenticate first.".to_string())
}

#[tauri::command]
pub async fn get_spotify_playlists() -> Result<Vec<SpotifyPlaylist>, String> {
    let access_token = get_spotify_token().await?;
    
    let client = reqwest::Client::new();
    let response: reqwest::Response = client
        .get("https://api.spotify.com/v1/me/playlists")
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch playlists: {}", e))?;
    
    if response.status().is_success() {
        let data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;
        
        let items = data["items"]
            .as_array()
            .ok_or("Missing items in response")?;
        
        let playlists: Vec<SpotifyPlaylist> = items
            .iter()
            .filter_map(|item| {
                let id = item["id"].as_str()?.to_string();
                let name = item["name"].as_str()?.to_string();
                let description = item["description"].as_str().map(String::from);
                let external_urls = item["external_urls"].clone();
                Some(SpotifyPlaylist {
                    id,
                    name,
                    description,
                    external_urls,
                })
            })
            .collect();
        
        Ok(playlists)
    } else {
        let error_text: String = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        Err(format!("Failed to fetch playlists: {}", error_text))
    }
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

#[tauri::command]
pub async fn check_ytdlp_available() -> Result<bool, String> {
    // First try to check in system PATH
    let path_check = tokio::process::Command::new("yt-dlp")
        .arg("--version")
        .output()
        .await;

    if let Ok(output) = path_check {
        if output.status.success() {
            return Ok(true);
        }
    }

    // If not in PATH, check our installation directory
    let executable_name = if cfg!(target_os = "windows") { "yt-dlp.exe" } else { "yt-dlp" };
    
    let install_dir = dirs::data_dir()
        .or_else(|| dirs::home_dir())
        .ok_or("Failed to get user directory")?
        .join("whiplash");
    
    let install_path = install_dir.join(executable_name);
    
    if install_path.exists() {
        // Try to run it to verify it works
        let output = tokio::process::Command::new(&install_path)
            .arg("--version")
            .output()
            .await;

        match output {
            Ok(output) => Ok(output.status.success()),
            Err(_) => Ok(false)
        }
    } else {
        Ok(false)
    }
}

#[tauri::command]
pub async fn install_ytdlp() -> Result<String, String> {
    let (os, arch) = get_os_arch();
    
    let download_url = match (os.as_str(), arch.as_str()) {
        ("windows", "x86_64") => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe",
        ("linux", "x86_64") => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp",
        ("macos", "x86_64") | ("macos", "aarch64") => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos",
        _ => return Err("Unsupported platform".to_string())
    };

    let client = reqwest::Client::new();
    let response = client
        .get(download_url)
        .send()
        .await
        .map_err(|e| format!("Failed to download yt-dlp: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Failed to download yt-dlp: HTTP {}", response.status()));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read yt-dlp binary: {}", e))?;

    let executable_name = if os == "windows" { "yt-dlp.exe" } else { "yt-dlp" };
    
    // Get user's home directory or data directory for installation
    let install_dir = dirs::data_dir()
        .or_else(|| dirs::home_dir())
        .ok_or("Failed to get user directory")?
        .join("whiplash");
    
    std::fs::create_dir_all(&install_dir)
        .map_err(|e| format!("Failed to create install directory: {}", e))?;

    let install_path = install_dir.join(executable_name);
    
    std::fs::write(&install_path, bytes)
        .map_err(|e| format!("Failed to write yt-dlp binary: {}", e))?;

    // Make executable on Unix-like systems
    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&install_path)
            .map_err(|e| format!("Failed to get file permissions: {}", e))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&install_path, perms)
            .map_err(|e| format!("Failed to set executable permissions: {}", e))?;
    }

    Ok(format!("yt-dlp installed successfully to: {}", install_path.display()))
}

fn get_os_arch() -> (String, String) {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    (os.to_string(), arch.to_string())
}

#[tauri::command]
pub async fn logout_spotify() -> Result<String, String> {
    let cache_path = get_cache_path()?;
    
    if cache_path.exists() {
        std::fs::remove_file(&cache_path)
            .map_err(|e| format!("Failed to remove Spotify token file: {}", e))?;
        Ok("Successfully logged out from Spotify".to_string())
    } else {
        Ok("No Spotify session found".to_string())
    }
}

#[tauri::command]
pub async fn force_spotify_reauth() -> Result<String, String> {
    // Clear the cached token to force re-authentication
    let cache_path = get_cache_path()?;
    
    if cache_path.exists() {
        std::fs::remove_file(&cache_path)
            .map_err(|e| format!("Failed to remove Spotify token file: {}", e))?;
    }
    
    Ok("Token cleared. Please authenticate again using the main authentication flow.".to_string())
}

#[tauri::command]
pub async fn get_selected_watch_folder() -> Result<Option<i64>, String> {
    // This would typically read from a config file
    // For now, we'll return the first watch folder if available
    // In a real implementation, this should be stored in a config file
    Ok(None) // Placeholder - should be implemented with proper config storage
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SpotifyTrack {
    pub id: String,
    pub name: String,
    pub artists: Vec<String>,
    pub album: String,
    pub duration_ms: u32,
    pub external_urls: serde_json::Value,
}

#[tauri::command]
pub async fn get_spotify_playlist_tracks(playlist_id: String) -> Result<Vec<SpotifyTrack>, String> {
    let access_token = get_spotify_token().await?;
    
    // eprintln!("DEBUG: Attempting to fetch tracks for playlist ID: {}", playlist_id);
    
    let client = reqwest::Client::new();
    
    // First, get playlist info to check if we can access it
    let playlist_url = format!("https://api.spotify.com/v1/playlists/{}", playlist_id);
    // eprintln!("DEBUG: Getting playlist info from: {}", playlist_url);
    
    let playlist_response = client
        .get(&playlist_url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Failed to request playlist info: {}", e))?;

    // Declare is_public in wider scope to be available later
    let mut is_public = false;

    if playlist_response.status().is_success() {
        let playlist_data: serde_json::Value = playlist_response
            .json()
            .await
            .map_err(|e| format!("Failed to parse playlist info: {}", e))?;
        
        let playlist_name = playlist_data["name"].as_str().unwrap_or("Unknown");
        let playlist_owner = playlist_data["owner"]["display_name"].as_str().unwrap_or("Unknown");
        is_public = playlist_data["public"].as_bool().unwrap_or(false);
        
        // eprintln!("DEBUG: Playlist '{}' by '{}' (public: {})", playlist_name, playlist_owner, is_public);
    } else {
        let status = playlist_response.status();
        let error_text = playlist_response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        // eprintln!("DEBUG: Failed to get playlist info - Status: {}, Error: {}", status, error_text);
        
        if status.as_u16() == 403 {
            return Err(format!("Cannot access playlist info (403). This playlist may be private or you don't have permission to access it.\n\nError: {}", error_text));
        }
        return Err(format!("Failed to get playlist info: {}", error_text));
    }
    
    // Now try to get tracks
    let url = format!("https://api.spotify.com/v1/playlists/{}/items", playlist_id);
    // eprintln!("DEBUG: Making tracks request to: {}", url);
    // eprintln!("DEBUG: Token length: {}", access_token.len());
    // eprintln!("DEBUG: Token prefix: {}", &access_token[..10]);
    // eprintln!("DEBUG: curl -X GET \"{}\" -H \"Authorization: Bearer {}\"", url, access_token);
    
    let mut tracks = Vec::new();
    let mut offset = 0;
    let limit = 50;
    
    loop {
        let response: reqwest::Response = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Accept", "application/json")
            .query(&[("limit", limit.to_string()), ("offset", offset.to_string())])
            .send()
            .await
            .map_err(|e| format!("Failed to fetch playlist tracks: {}", e))?;
        
        if response.status().is_success() {
            let data: serde_json::Value = response
                .json()
                .await
                .map_err(|e| format!("Failed to parse response: {}", e))?;
            
            // eprintln!("DEBUG: Raw response keys: {:?}", data.as_object().map(|o| o.keys().collect::<Vec<_>>()));
            
            let items = data["items"]
                .as_array()
                .ok_or("Missing items in response")?;
            
            // eprintln!("DEBUG: Found {} items in response", items.len());
            
            if items.is_empty() {
                break;
            }
            
            // Log first item structure for debugging
            if let Some(first_item) = items.first() {
                // eprintln!("DEBUG: First item structure: {}", serde_json::to_string_pretty(first_item).unwrap_or_else(|_| "Failed to serialize".to_string()));
            }
            
            for item in items {
                // /items endpoint has structure: item.item (track data)
                if let Some(track) = item["item"].as_object() {
                    // Check if it's actually a track (not an episode)
                    if track.get("type").and_then(|t| t.as_str()) == Some("track") {
                        let id = track["id"]
                            .as_str()
                            .ok_or("Missing track id")?
                            .to_string();
                        
                        let name = track["name"]
                            .as_str()
                            .ok_or("Missing track name")?
                            .to_string();
                        
                        let artists: Vec<String> = track["artists"]
                            .as_array()
                            .ok_or("Missing artists")?
                            .iter()
                            .filter_map(|artist| artist["name"].as_str().map(String::from))
                            .collect();
                        
                        let album = track["album"]["name"]
                            .as_str()
                            .unwrap_or("Unknown Album")
                            .to_string();
                        
                        let duration_ms = track["duration_ms"]
                            .as_u64()
                            .unwrap_or(0) as u32;
                        
                        let external_urls = track["external_urls"].clone();
                        
                        tracks.push(SpotifyTrack {
                            id,
                            name,
                            artists,
                            album,
                            duration_ms,
                            external_urls,
                        });
                    }
                }
            }
            
            offset += limit;
            
            // Check if there are more tracks
            let total = data["total"]
                .as_u64()
                .unwrap_or(0);
            if offset >= total as usize {
                break;
            }
        } else {
            let status = response.status();
            let error_text: String = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            
            // eprintln!("DEBUG: HTTP Status: {}", status);
            // eprintln!("DEBUG: Error response: {}", error_text);
            
            if status.as_u16() == 403 {
                // Check if we successfully got playlist info and it's a public playlist
                // If so, this might be a permissions issue with the items endpoint specifically
                // Don't force logout for public playlists where info was accessible
                if is_public {
                    return Err(format!("Cannot access playlist tracks (403). The playlist appears to be public but track access is restricted.\n\nThis could be due to:\n1. Regional restrictions\n2. Playlist owner's privacy settings\n3. Content licensing restrictions\n\nError details: {}", error_text));
                } else {
                    return Err(format!("Access denied (403). This usually means:\n1. The token has expired - try re-authenticating\n2. The playlist is private and not accessible\n3. Missing required permissions\n\nError details: {}", error_text));
                }
            } else {
                return Err(format!("Failed to fetch playlist tracks (HTTP {}): {}", status, error_text));
            }
        }
    }
    
    Ok(tracks)
}

#[tauri::command]
pub async fn get_spotify_saved_tracks() -> Result<Vec<SpotifyTrack>, String> {
    let access_token = get_spotify_token().await?;
    
    let client = reqwest::Client::new();
    
    // Get user's saved tracks
    let url = "https://api.spotify.com/v1/me/tracks";
    
    let mut tracks = Vec::new();
    let mut offset = 0;
    let limit = 50;
    
    loop {
        let response: reqwest::Response = client
            .get(url)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Accept", "application/json")
            .query(&[("limit", limit.to_string()), ("offset", offset.to_string())])
            .send()
            .await
            .map_err(|e| format!("Failed to request saved tracks: {}", e))?;

        if response.status().is_success() {
            let data: serde_json::Value = response
                .json()
                .await
                .map_err(|e| format!("Failed to parse response: {}", e))?;
            
            let items = data["items"]
                .as_array()
                .ok_or("Invalid response format")?
                .iter()
                .filter_map(|item| {
                    let track = item.get("track")?;
                    Some(SpotifyTrack {
                        id: track["id"].as_str()?.to_string(),
                        name: track["name"].as_str()?.to_string(),
                        artists: track["artists"]
                            .as_array()?
                            .iter()
                            .filter_map(|artist| artist["name"].as_str().map(|s| s.to_string()))
                            .collect(),
                        album: track["album"]["name"].as_str().unwrap_or("Unknown").to_string(),
                        duration_ms: track["duration_ms"].as_u64().unwrap_or(0) as u32,
                        external_urls: track["external_urls"].clone(),
                    })
                })
                .collect::<Vec<_>>();
            
            tracks.extend(items);
            
            let total = data["total"]
                .as_u64()
                .unwrap_or(0);
            if offset >= total as usize {
                break;
            }
            offset += limit;
        } else {
            let status = response.status();
            let error_text: String = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            
            if status.as_u16() == 403 {
                return Err(format!("Access denied (403). This usually means:\n1. The token has expired - try re-authenticating\n2. Missing required permissions\n\nError details: {}", error_text));
            } else {
                return Err(format!("Failed to fetch saved tracks (HTTP {}): {}", status, error_text));
            }
        }
    }
    
    Ok(tracks)
}

#[tauri::command]
pub async fn search_youtube_video(query: String) -> Result<String, String> {
    let client = reqwest::Client::new();
    
    // Format the query for YouTube search
    let formatted_query = query.split_whitespace().collect::<Vec<_>>().join("+");
    let url = format!("https://www.youtube.com/results?search_query={}", formatted_query);
    
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to search YouTube: {}", e))?;
    
    if response.status().is_success() {
        let html = response
            .text()
            .await
            .map_err(|e| format!("Failed to get response text: {}", e))?;
        
        // Extract video IDs using regex
        use regex::Regex;
        let re = Regex::new(r"watch\?v=(\S{11})")
            .map_err(|e| format!("Failed to compile regex: {}", e))?;
        
        let captures: Vec<_> = re.captures_iter(&html).collect();
        
        if let Some(capture) = captures.first() {
            if let Some(video_id) = capture.get(1) {
                return Ok(video_id.as_str().to_string());
            }
        }
        
        Err("No video found".to_string())
    } else {
        Err(format!("YouTube search failed: HTTP {}", response.status()))
    }
}

#[tauri::command]
pub async fn download_audio_from_youtube(video_id: String, output_path: String) -> Result<String, String> {
    let video_url = format!("https://www.youtube.com/watch?v={}", video_id);
    
    // Get the yt-dlp executable path
    let executable_name = if cfg!(target_os = "windows") { "yt-dlp.exe" } else { "yt-dlp" };
    
    let install_dir = dirs::data_dir()
        .or_else(|| dirs::home_dir())
        .ok_or("Failed to get user directory")?
        .join("whiplash");
    
    let ytdlp_path = install_dir.join(executable_name);
    
    if !ytdlp_path.exists() {
        return Err("yt-dlp is not installed. Please install it first.".to_string());
    }
    
    // Use yt-dlp to download audio
    let output = tokio::process::Command::new(&ytdlp_path)
        .arg("--format")
        .arg("m4a/bestaudio/best")
        .arg("--output")
        .arg(&output_path)
        .arg("--quiet")
        .arg("--no-warnings")
        .arg(&video_url)
        .output()
        .await
        .map_err(|e| format!("Failed to execute yt-dlp: {}", e))?;
    
    if output.status.success() {
        Ok(format!("Successfully downloaded to: {}", output_path))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("Download failed: {}", stderr))
    }
}

#[tauri::command]
pub async fn add_single_track_to_database(file_path: String, state: State<'_, AppState>) -> Result<Track, String> {
    let path = Path::new(&file_path);
    
    // Scan the specific file
    let scanned_track = scan_single_file(path)
        .ok_or("Failed to scan the specified file or file is not a valid audio file")?;
    
    let db_guard = (*state).database.lock().unwrap();
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let conn = db.get_connection();
    
    let now = Utc::now().timestamp();
    let duration = scanned_track.duration.map(|d| d as f64);
    
    // Check if track already exists
    let existing_track: Option<(i64, i32, Option<i64>, i64)> = conn
        .query_row(
            "SELECT id, play_count, last_played, date_added FROM tracks WHERE file_path = ?1",
            params![&scanned_track.file_path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .ok();
    
    let (id, play_count, last_played, date_added) = if let Some((id, pc, lp, da)) = existing_track {
        // Update existing track, preserving play_count, last_played, and date_added
        conn.execute(
            "UPDATE tracks SET folder_path = ?1, file_name = ?2, title = ?3, artist = ?4, album = ?5, genre = ?6, year = ?7, track_number = ?8, duration = ?9, is_missing = 0, date_modified = ?10, rating = ?11 WHERE file_path = ?12",
            params![
                scanned_track.folder_path,
                scanned_track.file_name,
                scanned_track.title,
                scanned_track.artist,
                scanned_track.album,
                scanned_track.genre,
                scanned_track.year,
                scanned_track.track_number.map(|t| t as i32),
                duration,
                now,
                0,
                scanned_track.file_path,
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
                scanned_track.file_path,
                scanned_track.folder_path,
                scanned_track.file_name,
                scanned_track.title,
                scanned_track.artist,
                scanned_track.album,
                scanned_track.genre,
                scanned_track.year,
                scanned_track.track_number.map(|t| t as i32),
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
    
    Ok(Track {
        id,
        file_path: scanned_track.file_path,
        folder_path: scanned_track.folder_path,
        file_name: scanned_track.file_name,
        title: scanned_track.title,
        artist: scanned_track.artist,
        album: scanned_track.album,
        genre: scanned_track.genre,
        year: scanned_track.year,
        track_number: scanned_track.track_number.map(|t| t as i32),
        duration,
        play_count,
        date_added,
        last_played,
        is_missing: false,
        date_modified: Some(now),
        rating: 0,
    })
}

#[tauri::command]
pub async fn check_for_update(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    use tauri_plugin_updater::UpdaterExt;
    
    match app.updater() {
        Ok(updater) => {
            match updater.check().await {
                Ok(Some(update)) => {
                    Ok(serde_json::json!({
                        "available": true,
                        "version": update.version,
                        "body": update.body,
                        "date": update.date.map(|d| d.to_string())
                    }))
                }
                Ok(None) => {
                    Ok(serde_json::json!({
                        "available": false
                    }))
                }
                Err(e) => Err(format!("Failed to check for updates: {}", e))
            }
        }
        Err(e) => Err(format!("Failed to get updater: {}", e))
    }
}

#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_updater::UpdaterExt;
    
    match app.updater() {
        Ok(updater) => {
            match updater.check().await {
                Ok(Some(update)) => {
                    match update.download_and_install(
                        |chunk_length, content_length| {
                            let content_len = content_length.unwrap_or(1);
                            let progress = chunk_length as f64 / content_len as f64;
                            println!("Download progress: {:.2}%", progress * 100.0);
                        },
                        || {
                            println!("Download finished");
                        },
                    ).await {
                        Ok(_) => {
                            app.restart();
                        }
                        Err(e) => Err(format!("Failed to install update: {}", e))
                    }
                }
                Ok(None) => Err("No update available".to_string()),
                Err(e) => Err(format!("Failed to check for updates before install: {}", e))
            }
        }
        Err(e) => Err(format!("Failed to get updater: {}", e))
    }
}

#[tauri::command]
pub fn get_current_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
