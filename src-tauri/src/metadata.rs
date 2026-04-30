use audiotags::Tag;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrackMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub track_number: Option<u32>,
    pub duration: Option<f64>,
}

pub fn extract_metadata(path: &Path) -> Result<TrackMetadata, Box<dyn std::error::Error>> {
    if let Ok(tag) = Tag::default().read_from_path(path) {
        let year = tag.year().map(|y| y as i32);
        let track_number = tag.track_number().map(|t| t as u32);
        
        return Ok(TrackMetadata {
            title: tag.title().map(|s| s.to_string()),
            artist: tag.artist().map(|s| s.to_string()),
            album: None, // audiotags Album type is complex, skip for now
            genre: tag.genre().map(|s| s.to_string()),
            year,
            track_number,
            duration: None,
        });
    }

    // Return empty metadata if extraction fails
    Ok(TrackMetadata {
        title: None,
        artist: None,
        album: None,
        genre: None,
        year: None,
        track_number: None,
        duration: None,
    })
}

pub fn write_metadata(path: &Path, metadata: &TrackMetadata) -> Result<(), Box<dyn std::error::Error>> {
    let mut tag = Tag::default().read_from_path(path)?;
    let path_str = path.to_str().ok_or("Invalid path")?;

    if let Some(title) = &metadata.title {
        tag.set_title(title);
    }
    if let Some(artist) = &metadata.artist {
        tag.set_artist(artist);
    }
    // Skip album - audiotags Album type is complex
    if let Some(genre) = &metadata.genre {
        tag.set_genre(genre);
    }
    if let Some(year) = metadata.year {
        tag.set_year(year);
    }
    if let Some(track_number) = metadata.track_number {
        tag.set_track_number(track_number as u16);
    }

    tag.write_to_path(path_str)?;
    Ok(())
}
