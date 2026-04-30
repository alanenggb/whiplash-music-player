export interface Track {
	id: number;
	file_path: string;
	folder_path: string;
	file_name: string;
	title: string | null;
	artist: string | null;
	album: string | null;
	genre: string | null;
	year: number | null;
	track_number: number | null;
	duration: number | null;
	play_count: number;
	date_added: number;
	last_played: number | null;
	is_missing: boolean;
	date_modified: number | null;
	rating: number;
}

export interface Tag {
	id: number;
	name: string;
}

export interface Playlist {
	id: number;
	name: string;
	date_created: number;
}

export interface WatchFolder {
	id: number;
	path: string;
	last_scanned: number | null;
	is_disabled: boolean;
}

export interface Device {
	id: number;
	path: string;
	name: string;
}

export interface Drive {
	letter: string;
	name: string | null;
}

export interface Folder {
	path: string;
	name: string;
}

export interface TrackMetadata {
	title: string | null;
	artist: string | null;
	album: string | null;
	genre: string | null;
	year: number | null;
	track_number: number | null;
	duration: number | null;
}
