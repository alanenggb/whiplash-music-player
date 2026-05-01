use crate::metadata::extract_metadata;
use crate::audio::AudioPlayer;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScannedTrack {
    pub file_path: String,
    pub folder_path: String,
    pub file_name: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub track_number: Option<u32>,
    pub duration: Option<f64>,
}

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "oga", "wav", "m4a", "aac", "wma", "ape", "mpc", "wavpack", "wv"
];

pub fn is_audio_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        let ext_lower = ext.to_string_lossy().to_lowercase();
        AUDIO_EXTENSIONS.contains(&ext_lower.as_str())
    } else {
        false
    }
}

pub fn scan_directory(path: &Path) -> Vec<ScannedTrack> {
    let mut tracks = Vec::new();

    for entry in WalkDir::new(path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let file_path = entry.path();
        
        if file_path.is_file() && is_audio_file(file_path) {
            if let Ok(metadata) = extract_metadata(file_path) {
                let duration = AudioPlayer::get_duration(file_path).ok();
                
                let folder_path = file_path
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                
                let file_name = file_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                tracks.push(ScannedTrack {
                    file_path: file_path.to_string_lossy().to_string(),
                    folder_path,
                    file_name,
                    title: metadata.title,
                    artist: metadata.artist,
                    album: metadata.album,
                    genre: metadata.genre,
                    year: metadata.year,
                    track_number: metadata.track_number,
                    duration,
                });
            }
        }
    }

    tracks
}

pub fn scan_single_file(file_path: &Path) -> Option<ScannedTrack> {
    if !file_path.is_file() || !is_audio_file(file_path) {
        return None;
    }
    
    if let Ok(metadata) = extract_metadata(file_path) {
        let duration = AudioPlayer::get_duration(file_path).ok();
        
        let folder_path = file_path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        
        let file_name = file_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        Some(ScannedTrack {
            file_path: file_path.to_string_lossy().to_string(),
            folder_path,
            file_name,
            title: metadata.title,
            artist: metadata.artist,
            album: metadata.album,
            genre: metadata.genre,
            year: metadata.year,
            track_number: metadata.track_number,
            duration,
        })
    } else {
        None
    }
}

#[allow(dead_code)]
pub fn scan_with_ignore(path: &Path, ignore_file: Option<&Path>) -> Vec<ScannedTrack> {
    let mut tracks = Vec::new();
    let mut walker = WalkBuilder::new(path);
    walker.follow_links(true);

    if let Some(ignore_path) = ignore_file {
        walker.add_ignore(ignore_path);
    }

    for entry in walker.build().filter_map(|e: Result<_, _>| e.ok()) {
        let file_path = entry.path();
        
        if file_path.is_file() && is_audio_file(file_path) {
            if let Ok(metadata) = extract_metadata(file_path) {
                let duration = AudioPlayer::get_duration(file_path).ok();
                
                let folder_path = file_path
                    .parent()
                    .map(|p: &std::path::Path| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                
                let file_name = file_path
                    .file_name()
                    .map(|n: &std::ffi::OsStr| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                tracks.push(ScannedTrack {
                    file_path: file_path.to_string_lossy().to_string(),
                    folder_path,
                    file_name,
                    title: metadata.title,
                    artist: metadata.artist,
                    album: metadata.album,
                    genre: metadata.genre,
                    year: metadata.year,
                    track_number: metadata.track_number,
                    duration,
                });
            }
        }
    }

    tracks
}
