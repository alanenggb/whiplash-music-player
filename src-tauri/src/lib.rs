mod audio;
mod commands;
mod database;
mod metadata;
mod scanner;

use commands::{AppState, initialize_database};
use std::sync::{Arc, Mutex};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let audio_player = Arc::new(Mutex::new(audio::AudioPlayer::new().expect("Failed to initialize audio player")));
    let database = Arc::new(Mutex::new(None));

    let app_state = AppState {
        audio_player,
        database,
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            initialize_database,
            commands::scan_folder,
            commands::get_all_tracks,
            commands::search_tracks,
            commands::play_track,
            commands::pause_playback,
            commands::resume_playback,
            commands::stop_playback,
            commands::set_volume,
            commands::seek,
            commands::get_frequency_data,
            commands::update_frequency_data,
            commands::is_playing,
            commands::update_track_metadata,
            commands::increment_play_count,
            commands::add_tag,
            commands::get_all_tags,
            commands::add_tag_to_track,
            commands::remove_tag_from_track,
            commands::get_tracks_by_tag,
            commands::create_playlist,
            commands::get_all_playlists,
            commands::add_track_to_playlist,
            commands::is_track_in_playlist,
            commands::get_playlist_track_count,
            commands::delete_playlist,
            commands::get_playlist_tracks,
            commands::remove_track_from_playlist,
            commands::reveal_in_file_explorer,
            commands::add_watch_folder,
            commands::get_watch_folders,
            commands::remove_watch_folder,
            commands::toggle_watch_folder,
            commands::mark_missing_tracks,
            commands::remove_missing_tracks,
            commands::get_recently_added,
            commands::get_never_played,
            commands::get_most_played,
            commands::get_recently_played,
            commands::get_recently_modified,
            commands::get_top_rated,
            commands::get_tracks_not_in_playlist,
            commands::open_devtools,
            commands::get_drives,
            commands::get_drive_folders,
            commands::add_device,
            commands::remove_device,
            commands::get_devices,
            commands::check_device_reachable,
            commands::get_device_tracks,
            commands::copy_file_to_device,
            commands::authenticate_spotify,
            commands::get_spotify_token,
            commands::get_spotify_playlists,
            commands::get_spotify_playlist_tracks,
            commands::get_spotify_saved_tracks,
            commands::search_youtube_video,
            commands::download_audio_from_youtube,
            commands::check_ytdlp_available,
            commands::install_ytdlp,
            commands::logout_spotify,
            commands::force_spotify_reauth,
            commands::get_selected_watch_folder,
            commands::add_single_track_to_database,
            commands::check_for_update,
            commands::install_update,
            commands::get_current_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
