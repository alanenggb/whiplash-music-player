<script lang="ts">
	import { invoke } from "@tauri-apps/api/core";
	import { open } from "@tauri-apps/plugin-dialog";
	import { getCurrentWindow } from "@tauri-apps/api/window";
	import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
	import { AudioProcessor } from "$lib/audioProcessor";
	import AudioVisualizer from "$lib/components/AudioVisualizer.svelte";
	import Equalizer from "$lib/components/Equalizer.svelte";
	import ConfigDialog from "$lib/components/ConfigDialog.svelte";
	import type { Track, Playlist, Device } from "$lib/types";

	let audioProcessor = new AudioProcessor();
	let tracks = $state<Track[]>([]);
	let currentTrack = $state<Track | null>(null);
	let isPlaying = $state(false);
	let isStopped = $state(false);
	let volume = $state(1.0);
	let volumeBeforeMute = $state(1.0);
	let currentTime = $state(0);
	let progress = $state(0);
	let isMuted = $state(false);
	let searchQuery = $state("");
	let vizType = $state<"bars" | "wave" | "circle">("bars");
	let dbInitialized = $state(false);
	let showEqualizer = $state(false);
	let currentPlaylist = $state<Track[]>([]);
	let playbackStartTime: number | null = null;
	let progressInterval: ReturnType<typeof setInterval> | null = null;
	let analyserNode = $state<AnalyserNode | null>(null);
	let showConfig = $state(false);

	// Library filter state
	let selectedFilter = $state<"all" | "most_played" | "recently_added" | "recently_modified" | "recently_played" | "never_played" | "top_rated" | "playlist" | "the_void" | "device">("all");
	let selectedPlaylistId = $state<number | null>(null);
	let playlists = $state<Playlist[]>([]);
	let libraryTracks = $state<Track[]>([]);
	let sortColumn = $state<"title" | "artist" | "album" | "track_number" | "duration" | "play_count" | null>(null);
	let sortDirection = $state<"asc" | "desc">("asc");

	// Device management state
	let devices = $state<Device[]>([]);
	let availableDevices = $state<Device[]>([]); // Only reachable devices
	let selectedDeviceId = $state<number | null>(null);
	let devicesSectionCollapsed = $state(false);
	let deviceCheckInterval: ReturnType<typeof setInterval> | null = null;

	// Context menu state
	let contextMenuVisible = $state(false);
	let contextMenuX = $state(0);
	let contextMenuY = $state(0);
	let contextMenuTrack = $state<Track | null>(null);
	let playlistSubmenuVisible = $state(false);
	let deviceSubmenuVisible = $state(false);
	let newPlaylistName = $state("");
	let submenuTimeout: ReturnType<typeof setTimeout> | null = null;
	let deviceSubmenuTimeout: ReturnType<typeof setTimeout> | null = null;
	let trackPlaylistMembership = $state<Record<number, boolean>>({});
	let submenuMaxHeight = $state(400);

	// Playlist context menu state
	let playlistContextMenuVisible = $state(false);
	let playlistContextMenuX = $state(0);
	let playlistContextMenuY = $state(0);
	let contextMenuPlaylist = $state<Playlist | null>(null);

	// Delete confirmation dialog state
	let showDeleteDialog = $state(false);
	let playlistToDelete = $state<Playlist | null>(null);
	let playlistTrackCount = $state(0);

	// Playlist panel state
	let playlistPanelVisible = $state(true);

	// Library header playlist dropdown state
	let playlistDropdownVisible = $state(false);
	let playlistCreationError = $state(false);

	// Library left section collapse state
	let filterSectionCollapsed = $state(false);
	let playlistSectionCollapsed = $state(false);

	// Context menu positioning
	let contextMenuPositionFromBottom = $state(false);
	let playlistContextMenuPositionFromBottom = $state(false);

	// Inline playlist creation state
	let showInlinePlaylistInput = $state(false);
	let inlinePlaylistName = $state("");
	let inlinePlaylistCreationError = $state(false);

	// Play count tracking state
	let playCountIncremented = $state(false);
	let totalPlayedTime = $state(0);

	// Load saved database path on startup
	async function loadSavedDatabase() {
		const savedPath = localStorage.getItem("databasePath");
		if (savedPath) {
			console.log("Found saved database path:", savedPath);
			try {
				await invoke("initialize_database", { dbPath: savedPath });
				dbInitialized = true;
				await loadTracks();
				await loadPlaylists();
				await loadDevices();
				// Start 5-second interval check for device reachability
				deviceCheckInterval = setInterval(checkDeviceReachability, 5000);
				console.log(
					"Database initialized successfully from saved path",
				);
			} catch (error) {
				console.error("Failed to load saved database:", error);
				// Clear the saved path if it's invalid
				localStorage.removeItem("databasePath");
			}
		} else {
			console.log("No saved database path found");
		}
	}

	// Save window position and size
	async function saveWindowSettings() {
		try {
			const window = getCurrentWindow();
			const position = await window.outerPosition();
			const size = await window.outerSize();

			localStorage.setItem(
				"windowPosition",
				JSON.stringify({ x: position.x, y: position.y }),
			);
			localStorage.setItem(
				"windowSize",
				JSON.stringify({ width: size.width, height: size.height }),
			);
			console.log("Saved window settings:", { position, size });
		} catch (error) {
			console.error("Failed to save window settings:", error);
		}
	}

	// Load and restore window position and size
	async function restoreWindowSettings() {
		try {
			const savedPosition = localStorage.getItem("windowPosition");
			const savedSize = localStorage.getItem("windowSize");

			const window = getCurrentWindow();

			if (savedSize) {
				const { width, height } = JSON.parse(savedSize);
				await window.setSize(new LogicalSize(width, height));
				console.log("Restored window size:", { width, height });
			}

			if (savedPosition) {
				const { x, y } = JSON.parse(savedPosition);
				await window.setPosition(new LogicalPosition(x, y));
				console.log("Restored window position:", { x, y });
			}
		} catch (error) {
			console.error("Failed to restore window settings:", error);
		}
	}

	// Setup window event listeners
	async function setupWindowListeners() {
		try {
			const window = getCurrentWindow();

			// Listen to resize and move events
			await window.onResized(() => saveWindowSettings());
			await window.onMoved(() => saveWindowSettings());

			console.log("Window listeners setup complete");
		} catch (error) {
			console.error("Failed to setup window listeners:", error);
		}
	}

	// Load saved volume on startup
	async function loadSavedVolume() {
		const savedVolume = localStorage.getItem("volume");
		if (savedVolume) {
			volume = parseFloat(savedVolume);
			await setVolume(volume);
			console.log("Restored volume:", volume);
		}
	}

	// Call on mount
	(async () => {
		await loadSavedDatabase();
		await restoreWindowSettings();
		await setupWindowListeners();
		await loadSavedVolume();
	})();

	async function initializeDatabase() {
		try {
			const folder = await open({
				directory: true,
				multiple: false,
				title: "Select folder for database",
			});

			if (folder) {
				const dbPath = `${folder}/whiplash.db`;
				await invoke("initialize_database", { dbPath });
				localStorage.setItem("databasePath", dbPath);
				dbInitialized = true;
				await loadTracks();
				await loadPlaylists();
				await loadDevices();
				// Start 5-second interval check for device reachability
				deviceCheckInterval = setInterval(checkDeviceReachability, 5000);
			}
		} catch (error) {
			console.error("Failed to initialize database:", error);
		}
	}

	async function loadTracks() {
		try {
			tracks = await invoke<Track[]>("get_all_tracks");
			libraryTracks = tracks;
			console.log(
				"Loaded tracks from database:",
				tracks.length,
				"tracks",
			);
			console.log("First track:", tracks[0]);
		} catch (error) {
			console.error("Failed to load tracks:", error);
		}
	}

	async function loadPlaylists() {
		try {
			playlists = await invoke<Playlist[]>("get_all_playlists");
			console.log("Loaded playlists:", playlists.length);
		} catch (error) {
			console.error("Failed to load playlists:", error);
		}
	}

	async function loadDevices() {
		try {
			devices = await invoke<Device[]>("get_devices");
			await checkDeviceReachability();
		} catch (error) {
			console.error("Failed to load devices:", error);
		}
	}

	async function checkDeviceReachability() {
		const reachable: Device[] = [];
		for (const device of devices) {
			try {
				const isReachable = await invoke<boolean>("check_device_reachable", { path: device.path });
				if (isReachable) {
					reachable.push(device);
				}
			} catch (error) {
				console.error(`Failed to check reachability for device ${device.name}:`, error);
			}
		}
		availableDevices = reachable;
	}

	async function selectDevice(device: Device) {
		selectedDeviceId = device.id;
		selectedFilter = "device";
		selectedPlaylistId = null;
		try {
			libraryTracks = await invoke<Track[]>("get_device_tracks", { devicePath: device.path });
			tracks = libraryTracks;
		} catch (error) {
			console.error("Failed to load device tracks:", error);
			libraryTracks = [];
			tracks = [];
		}
	}

	async function copyTrackToDevice(track: Track, device: Device) {
		try {
			const destination = await invoke<string>("copy_file_to_device", {
				sourcePath: track.file_path,
				devicePath: device.path,
			});
			console.log(`Copied track to device: ${destination}`);
			// Optionally show a success message to the user
		} catch (error) {
			console.error("Failed to copy track to device:", error);
			alert(`Failed to copy track to ${device.name}: ${error}`);
		}
	}

	async function applyFilter() {
		try {
			let filteredTracks: Track[] = [];
			switch (selectedFilter) {
				case "all":
					filteredTracks = await invoke<Track[]>("get_all_tracks");
					break;
				case "most_played":
					filteredTracks = await invoke<Track[]>("get_most_played", { limit: 100 });
					break;
				case "recently_added":
					filteredTracks = await invoke<Track[]>("get_recently_added", { limit: 100 });
					break;
				case "recently_modified":
					filteredTracks = await invoke<Track[]>("get_recently_modified", { limit: 100 });
					break;
				case "recently_played":
					filteredTracks = await invoke<Track[]>("get_recently_played", { limit: 100 });
					break;
				case "never_played":
					filteredTracks = await invoke<Track[]>("get_never_played");
					break;
				case "top_rated":
					filteredTracks = await invoke<Track[]>("get_top_rated", { limit: 100 });
					break;
				case "the_void":
					filteredTracks = await invoke<Track[]>("get_tracks_not_in_playlist");
					break;
				case "playlist":
					if (selectedPlaylistId !== null) {
						filteredTracks = await invoke<Track[]>("get_playlist_tracks", { playlistId: selectedPlaylistId });
					}
					break;
				case "device":
					if (selectedDeviceId !== null) {
						const device = devices.find(d => d.id === selectedDeviceId);
						if (device) {
							filteredTracks = await invoke<Track[]>("get_device_tracks", { devicePath: device.path });
						}
					}
					break;
			}
			libraryTracks = filteredTracks;
			if (sortColumn) {
				sortTracks(sortColumn);
			}
		} catch (error) {
			console.error("Failed to apply filter:", error);
		}
	}

	function sortTracks(column: "title" | "artist" | "album" | "track_number" | "duration" | "play_count") {
		if (sortColumn === column) {
			sortDirection = sortDirection === "asc" ? "desc" : "asc";
		} else {
			sortColumn = column;
			sortDirection = "asc";
		}

		libraryTracks.sort((a, b) => {
			let aVal = a[column];
			let bVal = b[column];

			if (aVal === null && bVal === null) return 0;
			if (aVal === null) return sortDirection === "asc" ? 1 : -1;
			if (bVal === null) return sortDirection === "asc" ? -1 : 1;

			if (typeof aVal === "string" && typeof bVal === "string") {
				return sortDirection === "asc"
					? aVal.localeCompare(bVal)
					: bVal.localeCompare(aVal);
			}

			if (typeof aVal === "number" && typeof bVal === "number") {
				return sortDirection === "asc" ? aVal - bVal : bVal - aVal;
			}

			return 0;
		});
	}

	async function addTrackToPlaylist(track: Track, playlistId: number) {
		try {
			await invoke("add_track_to_playlist", { playlistId, trackId: track.id });
			console.log("Added track to playlist");
			closeContextMenu();
		} catch (error) {
			console.error("Failed to add track to playlist:", error);
		}
	}

	async function createPlaylistAndAddTrack() {
		if (!contextMenuTrack || !newPlaylistName.trim()) return;
		const name = newPlaylistName.trim();

		// Check for duplicate playlist name
		const duplicate = playlists.find(p => p.name.toLowerCase() === name.toLowerCase());
		if (duplicate) {
			console.error("Playlist with this name already exists");
			return;
		}

		try {
			const playlist = await invoke("create_playlist", {
				name: name,
			}) as Playlist;
			await invoke("add_track_to_playlist", {
				playlistId: playlist.id,
				trackId: contextMenuTrack.id,
			});
			await loadPlaylists();
			// Update membership for the new playlist
			trackPlaylistMembership[playlist.id] = true;
			newPlaylistName = "";
			closeContextMenu();
			console.log("Created playlist and added track");
		} catch (error) {
			console.error("Failed to create playlist:", error);
		}
	}

	async function toggleTrackInPlaylist(playlistId: number, isChecked: boolean) {
		if (!contextMenuTrack) return;
		try {
			if (isChecked) {
				await invoke("add_track_to_playlist", {
					playlistId: playlistId,
					trackId: contextMenuTrack.id,
				});
			} else {
				await invoke("remove_track_from_playlist", {
					playlistId: playlistId,
					trackId: contextMenuTrack.id,
				});
			}
			trackPlaylistMembership[playlistId] = isChecked;
		} catch (error) {
			console.error("Failed to toggle track in playlist:", error);
		}
	}

	function showContextMenu(event: MouseEvent, track: Track) {
		closePlaylistContextMenu();
		event.preventDefault();
		contextMenuTrack = track;
		contextMenuX = event.clientX;
		contextMenuVisible = true;
		playlistSubmenuVisible = false;

		// Fetch playlist membership for this track
		fetchPlaylistMembership(track.id);

		// Calculate available space for submenu
		const viewportHeight = window.innerHeight;
		const estimatedMenuHeight = 130; // Approximate height of context menu
		if (event.clientY + estimatedMenuHeight > viewportHeight) {
			// When menu is positioned from bottom, set fixed max-height
			submenuMaxHeight = 300;
		} else {
			// Normal calculation when menu is positioned from top
			const availableSpace = viewportHeight - event.clientY - 20; // 20px padding
			submenuMaxHeight = Math.min(400, availableSpace);
		}

		// Position context menu to avoid going off-screen
		if (event.clientY + estimatedMenuHeight > viewportHeight) {
			// Position menu from bottom (bottom-left pivot)
			contextMenuPositionFromBottom = true;
			contextMenuY = viewportHeight - event.clientY;
		} else {
			// Normal position: menu top at mouse position
			contextMenuPositionFromBottom = false;
			contextMenuY = event.clientY;
		}
	}

	async function fetchPlaylistMembership(trackId: number) {
		const membership: Record<number, boolean> = {};
		for (const playlist of playlists) {
			try {
				const isInPlaylist = await invoke("is_track_in_playlist", {
					playlistId: playlist.id,
					trackId: trackId,
				});
				membership[playlist.id] = isInPlaylist as boolean;
			} catch (error) {
				console.error("Failed to check playlist membership:", error);
				membership[playlist.id] = false;
			}
		}
		trackPlaylistMembership = membership;
	}

	function closeContextMenu() {
		contextMenuVisible = false;
		playlistSubmenuVisible = false;
		contextMenuTrack = null;
	}

	function showPlaylistContextMenu(event: MouseEvent, playlist: Playlist) {
		closeContextMenu();
		event.preventDefault();
		contextMenuPlaylist = playlist;
		playlistContextMenuX = event.clientX;
		playlistContextMenuVisible = true;

		// Position context menu to avoid going off-screen
		const viewportHeight = window.innerHeight;
		const estimatedMenuHeight = 80; // Approximate height of playlist context menu
		if (event.clientY + estimatedMenuHeight > viewportHeight) {
			// Position menu from bottom (bottom-left pivot)
			playlistContextMenuPositionFromBottom = true;
			playlistContextMenuY = viewportHeight - event.clientY;
		} else {
			// Normal position: menu top at mouse position
			playlistContextMenuPositionFromBottom = false;
			playlistContextMenuY = event.clientY;
		}
	}

	function closePlaylistContextMenu() {
		playlistContextMenuVisible = false;
		contextMenuPlaylist = null;
	}

	async function confirmDeletePlaylist() {
		if (!playlistToDelete) return;
		try {
			await invoke("delete_playlist", {
				playlistId: playlistToDelete.id,
			});
			await loadPlaylists();
			// If the deleted playlist was selected, reset the filter
			if (selectedFilter === "playlist" && selectedPlaylistId === playlistToDelete.id) {
				selectedFilter = "all";
				selectedPlaylistId = null;
				applyFilter();
			}
			showDeleteDialog = false;
			playlistToDelete = null;
			console.log("Playlist deleted successfully");
		} catch (error) {
			console.error("Failed to delete playlist:", error);
		}
	}

	async function initiateDeletePlaylist(playlist: Playlist) {
		try {
			const count = await invoke("get_playlist_track_count", {
				playlistId: playlist.id,
			}) as number;
			playlistToDelete = playlist;
			playlistTrackCount = count;
			showDeleteDialog = true;
			closePlaylistContextMenu();
		} catch (error) {
			console.error("Failed to get playlist track count:", error);
		}
	}

	async function playTrack(track: Track) {
		console.log("playTrack called for:", track.title || track.file_name);
		try {
			// Stop any existing progress timer
			if (progressInterval) {
				clearInterval(progressInterval);
				progressInterval = null;
			}

			// Use Web Audio API for playback and FFT
			await audioProcessor.loadAudioFromPath(track.file_path);
			audioProcessor.play();
			analyserNode = audioProcessor.getAnalyser();

			currentTrack = track;
			isPlaying = true;
			isStopped = false;

			// Reset play count tracking for new track
			playCountIncremented = false;
			totalPlayedTime = 0;
			playbackStartTime = Date.now();

			// Apply current volume after starting playback
			await setVolume(volume);

			// Start progress timer
			progressInterval = setInterval(() => {
				if (isPlaying && currentTrack && playbackStartTime) {
					currentTime = audioProcessor.getCurrentTime();
					if (currentTrack.duration) {
						progress = currentTime / currentTrack.duration;
					}

					// Increment total played time (0.1 seconds per tick)
					totalPlayedTime += 0.1;

					// Check if track has played for at least 30 seconds and increment play count
					if (!playCountIncremented && totalPlayedTime >= 30) {
						incrementPlayCount(currentTrack.file_path);
						playCountIncremented = true;
					}
				}
			}, 100);

			console.log("Track started successfully via Web Audio API");
		} catch (error) {
			console.error("Failed to play track:", error);
			// Try to provide more details about the error
			if (error && typeof error === "object" && "message" in error) {
				console.error("Error details:", error.message);
			}
		}
	}

	async function togglePlayback() {
		if (!currentTrack) return;

		if (isStopped) {
			// Restart the track if it was stopped
			await playTrack(currentTrack);
			isStopped = false;
		} else if (isPlaying) {
			audioProcessor.pause();
			isPlaying = false;
			// Pause progress timer
			if (progressInterval) {
				clearInterval(progressInterval);
				progressInterval = null;
			}
		} else {
			audioProcessor.play();
			isPlaying = true;
			// Resume progress timer
			progressInterval = setInterval(() => {
				if (isPlaying && currentTrack) {
					currentTime = audioProcessor.getCurrentTime();
					if (currentTrack.duration) {
						progress = currentTime / currentTrack.duration;
					}

					// Increment total played time (0.1 seconds per tick)
					totalPlayedTime += 0.1;

					// Check if track has played for at least 30 seconds and increment play count
					if (!playCountIncremented && totalPlayedTime >= 30) {
						incrementPlayCount(currentTrack.file_path);
						playCountIncremented = true;
					}
				}
			}, 100);
		}
	}

	async function stopPlayback() {
		audioProcessor.stop();
		isPlaying = false;
		isStopped = true;
		currentTime = 0;
		progress = 0;
		// Stop progress timer
		if (progressInterval) {
			clearInterval(progressInterval);
			progressInterval = null;
		}
	}

	async function setVolume(value: number) {
		volume = value;
		audioProcessor.setVolume(value);
		localStorage.setItem("volume", value.toString());
		// If user adjusts volume while muted, unmute
		if (isMuted && value > 0) {
			isMuted = false;
		}
	}

	async function toggleMute() {
		if (isMuted) {
			// Unmute: restore previous volume
			await setVolume(volumeBeforeMute);
			isMuted = false;
		} else {
			// Mute: save current volume and set to 0
			volumeBeforeMute = volume;
			await setVolume(0);
			isMuted = true;
		}
	}

	async function seekTo(position: number) {
		if (!currentTrack) return;
		try {
			audioProcessor.seek(position);
			currentTime = position;
			if (currentTrack.duration) {
				progress = position / currentTrack.duration;
			}
		} catch (error) {
			console.error("Failed to seek:", error);
		}
	}

	async function searchTracks() {
		if (searchQuery.trim()) {
			// First get the filtered tracks based on current filter
			let filteredTracks: Track[] = [];
			switch (selectedFilter) {
				case "all":
					filteredTracks = await invoke<Track[]>("get_all_tracks");
					break;
				case "most_played":
					filteredTracks = await invoke<Track[]>("get_most_played", { limit: 100 });
					break;
				case "recently_added":
					filteredTracks = await invoke<Track[]>("get_recently_added", { limit: 100 });
					break;
				case "recently_modified":
					filteredTracks = await invoke<Track[]>("get_recently_modified", { limit: 100 });
					break;
				case "recently_played":
					filteredTracks = await invoke<Track[]>("get_recently_played", { limit: 100 });
					break;
				case "never_played":
					filteredTracks = await invoke<Track[]>("get_never_played");
					break;
				case "top_rated":
					filteredTracks = await invoke<Track[]>("get_top_rated", { limit: 100 });
					break;
				case "the_void":
					filteredTracks = await invoke<Track[]>("get_tracks_not_in_playlist");
					break;
				case "playlist":
					if (selectedPlaylistId !== null) {
						filteredTracks = await invoke<Track[]>("get_playlist_tracks", { playlistId: selectedPlaylistId });
					}
					break;
			}
			// Then filter by search query client-side
			const query = searchQuery.toLowerCase();
			libraryTracks = filteredTracks.filter(
				(track) =>
					track.title?.toLowerCase().includes(query) ||
					track.artist?.toLowerCase().includes(query) ||
					track.album?.toLowerCase().includes(query) ||
					track.file_name?.toLowerCase().includes(query) ||
					track.folder_path?.toLowerCase().includes(query)
			);
		} else {
			await applyFilter();
		}
	}

	function formatDuration(seconds?: number | null) {
		if (!seconds) return "0:00";
		const mins = Math.floor(seconds / 60);
		const secs = Math.floor(seconds % 60);
		return `${mins}:${secs.toString().padStart(2, "0")}`;
	}

	function addToPlaylist(track: Track) {
		currentPlaylist = [...currentPlaylist, track];
	}

	function removeFromPlaylist(index: number) {
		currentPlaylist = currentPlaylist.filter((_, i) => i !== index);
	}

	function clearPlaylist() {
		currentPlaylist = [];
	}

	async function playNext() {
		if (currentPlaylist.length === 0) return;
		if (!currentTrack) {
			await playTrack(currentPlaylist[0]);
			return;
		}
		const currentIndex = currentPlaylist.findIndex(
			(t) => t.id === currentTrack!.id,
		);
		const nextIndex = (currentIndex + 1) % currentPlaylist.length;
		await playTrack(currentPlaylist[nextIndex]);
	}

	async function playPrevious() {
		if (currentPlaylist.length === 0) return;
		if (!currentTrack) {
			await playTrack(currentPlaylist[0]);
			return;
		}
		const currentIndex = currentPlaylist.findIndex(
			(t) => t.id === currentTrack!.id,
		);
		const prevIndex =
			currentIndex === 0 ? currentPlaylist.length - 1 : currentIndex - 1;
		await playTrack(currentPlaylist[prevIndex]);
	}

	async function restartTrack() {
		if (!currentTrack) return;
		await playTrack(currentTrack);
	}

	async function incrementPlayCount(filePath: string) {
		try {
			await invoke("increment_play_count", { filePath });
			console.log("Play count incremented for:", filePath);
		} catch (error) {
			console.error("Failed to increment play count:", error);
		}
	}

	async function addAllVisibleToPlaylist(playlistId: number) {
		try {
			let addedCount = 0;
			for (const track of libraryTracks) {
				await invoke("add_track_to_playlist", {
					playlistId: playlistId,
					trackId: track.id,
				});
				addedCount++;
			}
			console.log(`Added ${addedCount} tracks to playlist`);
		} catch (error) {
			console.error("Failed to add tracks to playlist:", error);
		}
	}

	async function createPlaylistAndAddAll() {
		if (!newPlaylistName || newPlaylistName.trim() === "") return;
		// Check for duplicate playlist name
		const duplicateName = playlists.some(p => p.name.toLowerCase() === newPlaylistName.toLowerCase());
		if (duplicateName) {
			console.error("Playlist with this name already exists");
			playlistCreationError = true;
			setTimeout(() => playlistCreationError = false, 2000);
			return;
		}
		try {
			await invoke("create_playlist", { name: newPlaylistName });
			console.log("Created playlist:", newPlaylistName);
			await loadPlaylists();
			newPlaylistName = "";
		} catch (error) {
			console.error("Failed to create playlist:", error);
			playlistCreationError = true;
			setTimeout(() => playlistCreationError = false, 2000);
		}
	}

	async function createInlinePlaylist() {
		if (!inlinePlaylistName || inlinePlaylistName.trim() === "") return;
		const name = inlinePlaylistName.trim();

		// Check for duplicate playlist name
		const duplicate = playlists.find(p => p.name.toLowerCase() === name.toLowerCase());
		if (duplicate) {
			console.error("Playlist with this name already exists");
			inlinePlaylistCreationError = true;
			setTimeout(() => inlinePlaylistCreationError = false, 2000);
			return;
		}

		try {
			await invoke("create_playlist", { name });
			console.log("Created playlist:", name);
			await loadPlaylists();
			showInlinePlaylistInput = false;
			inlinePlaylistName = "";
			inlinePlaylistCreationError = false;
		} catch (error) {
			console.error("Failed to create playlist:", error);
			inlinePlaylistCreationError = true;
			setTimeout(() => inlinePlaylistCreationError = false, 2000);
		}
	}

	function cancelInlinePlaylist() {
		showInlinePlaylistInput = false;
		inlinePlaylistName = "";
		inlinePlaylistCreationError = false;
	}
</script>
<svelte:window oncontextmenu={(e) => e.preventDefault()} onclick={() => { closeContextMenu(); closePlaylistContextMenu(); playlistDropdownVisible = false; }} onkeydown={(e) => {
	if (e.key === "Escape") {
		closeContextMenu();
		closePlaylistContextMenu();
		playlistDropdownVisible = false;
		showDeleteDialog = false;
		playlistToDelete = null;
		showConfig = false;
	}
}} />

<div class="app">

	{#if dbInitialized}
		<div class="main-content">
			<div class="library-column">
				<div class="player-area">
					<div class="player-left-column">
						<div class="current-song-section">
							<!-- Row 1: Song name and time -->
							<div class="song-info-row">
								<div class="song-name">
									{currentTrack?.title ||
										currentTrack?.file_name ||
										"---"}
								</div>
								<div class="song-time">
									{formatDuration(currentTime)} | {formatDuration(
										currentTrack?.duration,
									)}
								</div>
							</div>

							<!-- Row 2: Progress slider -->
							<div class="progress-row">
								<input
									type="range"
									min="0"
									max={currentTrack?.duration || 100}
									step="0.1"
									bind:value={currentTime}
									oninput={(e) =>
										seekTo(
											parseFloat(
												(e.target as HTMLInputElement)
													.value,
											),
										)}
									class="progress-slider"
								/>
							</div>

							<!-- Row 3: Playback controls and volume -->
							<div class="controls-row">
								<div class="playback-buttons">
									<button
										class="btn btn-control"
										onclick={playPrevious}>⏮</button
									>
									<button
										class="btn btn-control"
										onclick={restartTrack}>↺</button
									>
									<button
										onclick={togglePlayback}
										class="btn btn-control btn-large"
									>
										{isPlaying ? "⏸" : "▶"}
									</button>
									<button
										onclick={stopPlayback}
										class="btn btn-control"
									>
										⏹
									</button>
									<button
										class="btn btn-control"
										onclick={playNext}>⏭</button
									>
								</div>
								<div class="volume-section">
									<button
										class="btn btn-icon"
										onclick={toggleMute}
									>
										{isMuted ? "🔇" : "🔊"}
									</button>
									<input
										type="range"
										min="0"
										max="1"
										step="0.01"
										bind:value={volume}
										oninput={(e) =>
											setVolume(
												parseFloat(
													(
														e.target as HTMLInputElement
													).value,
												),
											)}
										class="volume-slider-compact"
									/>
								</div>
							</div>
						</div>
					</div>
					<div class="player-right-column">
						<div
							class="visualizer-section"
							onclick={() => {
								vizType =
									vizType === "bars"
										? "wave"
										: vizType === "wave"
											? "circle"
											: "bars";
							}}
							onkeydown={(e) => {
								if (e.key === "Enter" || e.key === " ") {
									vizType =
										vizType === "bars"
											? "wave"
											: vizType === "wave"
												? "circle"
												: "bars";
								}
							}}
							role="button"
							tabindex="0"
							style="cursor: pointer;"
							title="Click to change visualizer style"
						>
							<AudioVisualizer
								analyser={analyserNode}
								type={vizType}
								height={150}
							/>
						</div>
						<div class="player-buttons-section">
							<!-- Buttons will be added here later -->
						</div>
					</div>
				</div>

				<div class="library-section">
					<div class="library-left">
						<div class="filter-section">
							<!-- svelte-ignore a11y_click_events_have_key_events -->
							<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
							<h4 class="section-header" onclick={() => filterSectionCollapsed = !filterSectionCollapsed}>
								<span class="section-title">Library</span>
								<span class="chevron" class:collapsed={filterSectionCollapsed}>▼</span>
							</h4>
							<div class="filter-list" class:collapsed={filterSectionCollapsed}>
								<!-- svelte-ignore a11y_click_events_have_key_events -->
								<!-- svelte-ignore a11y_no_static_element_interactions -->
								<div
									class="filter-item"
									class:selected={selectedFilter === "all"}
									onclick={() => {
										selectedFilter = "all";
										selectedPlaylistId = null;
										if (searchQuery.trim()) {
											searchTracks();
										} else {
											applyFilter();
										}
									}}
								>
									All
								</div>
								<!-- svelte-ignore a11y_click_events_have_key_events -->
								<!-- svelte-ignore a11y_no_static_element_interactions -->
								<div
									class="filter-item"
									class:selected={selectedFilter === "most_played"}
									onclick={() => {
										selectedFilter = "most_played";
										selectedPlaylistId = null;
										if (searchQuery.trim()) {
											searchTracks();
										} else {
											applyFilter();
										}
									}}
								>
									Most Played
								</div>
								<!-- svelte-ignore a11y_click_events_have_key_events -->
								<!-- svelte-ignore a11y_no_static_element_interactions -->
								<div
									class="filter-item"
									class:selected={selectedFilter === "recently_added"}
									onclick={() => {
										selectedFilter = "recently_added";
										selectedPlaylistId = null;
										if (searchQuery.trim()) {
											searchTracks();
										} else {
											applyFilter();
										}
									}}
								>
									Recently Added
								</div>
								<!-- svelte-ignore a11y_click_events_have_key_events -->
								<!-- svelte-ignore a11y_no_static_element_interactions -->
								<div
									class="filter-item"
									class:selected={selectedFilter === "recently_modified"}
									onclick={() => {
										selectedFilter = "recently_modified";
										selectedPlaylistId = null;
										if (searchQuery.trim()) {
											searchTracks();
										} else {
											applyFilter();
										}
									}}
								>
									Recently Modified
								</div>
								<!-- svelte-ignore a11y_click_events_have_key_events -->
								<!-- svelte-ignore a11y_no_static_element_interactions -->
								<div
									class="filter-item"
									class:selected={selectedFilter === "recently_played"}
									onclick={() => {
										selectedFilter = "recently_played";
										selectedPlaylistId = null;
										if (searchQuery.trim()) {
											searchTracks();
										} else {
											applyFilter();
										}
									}}
								>
									Recently Played
								</div>
								<!-- svelte-ignore a11y_click_events_have_key_events -->
								<!-- svelte-ignore a11y_no_static_element_interactions -->
								<div
									class="filter-item"
									class:selected={selectedFilter === "the_void"}
									onclick={() => {
										selectedFilter = "the_void";
										selectedPlaylistId = null;
										if (searchQuery.trim()) {
											searchTracks();
										} else {
											applyFilter();
										}
									}}
								>
									The Void
								</div>
								<!-- svelte-ignore a11y_click_events_have_key_events -->
								<!-- svelte-ignore a11y_no_static_element_interactions -->
								<div
									class="filter-item"
									class:selected={selectedFilter === "never_played"}
									onclick={() => {
										selectedFilter = "never_played";
										selectedPlaylistId = null;
										if (searchQuery.trim()) {
											searchTracks();
										} else {
											applyFilter();
										}
									}}
								>
									Never Played
								</div>
								<!-- svelte-ignore a11y_click_events_have_key_events -->
								<!-- svelte-ignore a11y_no_static_element_interactions -->
								<div
									class="filter-item"
									class:selected={selectedFilter === "top_rated"}
									onclick={() => {
										selectedFilter = "top_rated";
										selectedPlaylistId = null;
										if (searchQuery.trim()) {
											searchTracks();
										} else {
											applyFilter();
										}
									}}
								>
									Top Rated
								</div>
							</div>
						</div>
						<div class="playlist-section">
							<!-- svelte-ignore a11y_click_events_have_key_events -->
							<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
							<h4 class="section-header" onclick={() => playlistSectionCollapsed = !playlistSectionCollapsed}>
								<span class="section-title">Playlists</span>
								<div class="section-header-actions">
									<button class="btn-add-playlist" onclick={(e) => { e.stopPropagation(); showInlinePlaylistInput = true; inlinePlaylistName = ""; inlinePlaylistCreationError = false; }}>+</button>
									<span class="chevron" class:collapsed={playlistSectionCollapsed}>▼</span>
								</div>
							</h4>
							{#if showInlinePlaylistInput}
								<div class="inline-playlist-input">
									<input
										type="text"
										placeholder="New playlist name..."
										bind:value={inlinePlaylistName}
										class:class:error={inlinePlaylistCreationError}
										onkeydown={(e) => {
											if (e.key === "Enter") {
												createInlinePlaylist();
											} else if (e.key === "Escape") {
												cancelInlinePlaylist();
											}
										}}
									/>
									<button class="btn-check" onclick={createInlinePlaylist}>✓</button>
									<button class="btn-cancel" onclick={cancelInlinePlaylist}>✕</button>
								</div>
							{/if}
							<div class="filter-list" class:collapsed={playlistSectionCollapsed}>
								{#each playlists as playlist}
									<!-- svelte-ignore a11y_click_events_have_key_events -->
									<!-- svelte-ignore a11y_no_static_element_interactions -->
									<div
										class="filter-item"
										class:selected={selectedFilter === "playlist" && selectedPlaylistId === playlist.id}
										onclick={() => {
											selectedFilter = "playlist";
											selectedPlaylistId = playlist.id;
											if (searchQuery.trim()) {
												searchTracks();
											} else {
												applyFilter();
											}
										}}
										oncontextmenu={(e) => showPlaylistContextMenu(e, playlist)}
									>
										{playlist.name}
									</div>
								{/each}
								{#if playlists.length === 0}
									<div class="empty-state">No playlists</div>
								{/if}
							</div>
						</div>
						<div class="devices-section">
							<!-- svelte-ignore a11y_click_events_have_key_events -->
							<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
							<h4 class="section-header" onclick={() => devicesSectionCollapsed = !devicesSectionCollapsed}>
								<span class="section-title">Devices</span>
								<span class="chevron" class:collapsed={devicesSectionCollapsed}>▼</span>
							</h4>
							<div class="filter-list" class:collapsed={devicesSectionCollapsed}>
								{#if availableDevices.length === 0}
									<div class="empty-state">No available devices</div>
								{:else}
									{#each availableDevices as device}
										<!-- svelte-ignore a11y_click_events_have_key_events -->
										<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
										<div
											class="filter-item"
											class:selected={selectedFilter === "device" && selectedDeviceId === device.id}
											onclick={() => selectDevice(device)}
										>
											<span class="device-icon">💾</span>
											<span class="device-name">{device.name}</span>
										</div>
									{/each}
								{/if}
							</div>
						</div>
					</div>
					<div class="library-right">
						<div class="library-header">
							<div class="search-box">
								<input
									type="text"
									placeholder="Search tracks..."
									bind:value={searchQuery}
									oninput={searchTracks}
									class="search-input"
								/>
								{#if searchQuery.trim()}
									<button
										class="btn-clear-search"
										onclick={() => {
											searchQuery = "";
											applyFilter();
										}}
									>
										✕
									</button>
								{/if}
							</div>
							<button
								class="btn btn-secondary"
								onclick={(e) => {
									e.stopPropagation();
									playlistDropdownVisible = !playlistDropdownVisible;
								}}
							>
								Add to Playlist
							</button>
							{#if playlistDropdownVisible}
								<div class="playlist-dropdown">
									<div class="playlist-dropdown-header">
										<span>Add to Playlist</span>
										<button
											class="btn-close"
											onclick={() => playlistDropdownVisible = false}
										>
											✕
										</button>
									</div>
									<div class="playlist-dropdown-list">
										{#each playlists as playlist}
											<div class="playlist-dropdown-item">
												<span class="playlist-name">{playlist.name}</span>
												<button
													class="btn btn-secondary btn-small"
													onclick={(e) => {
														e.stopPropagation();
														addAllVisibleToPlaylist(playlist.id);
													}}
												>
													+
												</button>
											</div>
										{/each}
										{#if playlists.length === 0}
											<div class="playlist-dropdown-item">
												<span class="playlist-name">No playlists</span>
											</div>
										{/if}
									</div>
									<div class="playlist-dropdown-footer">
										<div class="new-playlist">
											<input
												type="text"
												placeholder="New playlist name..."
												bind:value={newPlaylistName}
												onkeydown={(e) => {
													if (e.key === "Enter") {
														createPlaylistAndAddAll();
														e.stopPropagation();
													} else if (e.key === "Escape") {
														playlistDropdownVisible = false;
														e.stopPropagation();
													}
													e.stopPropagation();
												}}
												onclick={(e) => e.stopPropagation()}
												class="new-playlist-input"
												class:error={playlistCreationError}
											/>
											<button
												onclick={(e) => {
													e.stopPropagation();
													createPlaylistAndAddAll();
												}}
												class="btn-checkmark"
											>
												✓
											</button>
										</div>
									</div>
								</div>
							{/if}
						</div>
						<div class="track-table-container" onscroll={() => closeContextMenu()}>
							<table class="track-table">
								<thead>
									<tr>
										<th onclick={() => sortTracks("title")} class:sortable={true} class:sorted={sortColumn === "title"} style="width: 36%">
											Title {sortColumn === "title" ? (sortDirection === "asc" ? "▲" : "▼") : ""}
										</th>
										<th onclick={() => sortTracks("artist")} class:sortable={true} class:sorted={sortColumn === "artist"} style="width: 20%">
											Artist {sortColumn === "artist" ? (sortDirection === "asc" ? "▲" : "▼") : ""}
										</th>
										<th onclick={() => sortTracks("album")} class:sortable={true} class:sorted={sortColumn === "album"} style="width: 12%">
											Album {sortColumn === "album" ? (sortDirection === "asc" ? "▲" : "▼") : ""}
										</th>
										<th onclick={() => sortTracks("track_number")} class:sortable={true} class:sorted={sortColumn === "track_number"} style="width: 8%">
											Track# {sortColumn === "track_number" ? (sortDirection === "asc" ? "▲" : "▼") : ""}
										</th>
										<th onclick={() => sortTracks("duration")} class:sortable={true} class:sorted={sortColumn === "duration"} style="width: 10%">
											Length {sortColumn === "duration" ? (sortDirection === "asc" ? "▲" : "▼") : ""}
										</th>
										<th onclick={() => sortTracks("play_count")} class:sortable={true} class:sorted={sortColumn === "play_count"} style="width: 14%">
											Play Count {sortColumn === "play_count" ? (sortDirection === "asc" ? "▲" : "▼") : ""}
										</th>
									</tr>
								</thead>
								<tbody>
									{#each libraryTracks as track}
										<tr
											class:active={currentTrack?.id === track.id}
											onclick={() => playTrack(track)}
											oncontextmenu={(e) => showContextMenu(e, track)}
											onkeydown={(e) => {
												if (e.key === "Enter" || e.key === " ")
													playTrack(track);
											}}
											role="button"
											tabindex="0"
										>
											<td class="track-title-cell" title={track.title || track.file_name}>{track.title || track.file_name}</td>
											<td title={track.artist || "Unknown"}>{track.artist || "Unknown"}</td>
											<td title={track.album || "Unknown"}>{track.album || "Unknown"}</td>
											<td title={String(track.track_number || "-")}>{track.track_number || "-"}</td>
											<td title={formatDuration(track.duration)}>{formatDuration(track.duration)}</td>
											<td title={String(track.play_count)}>{track.play_count}</td>
										</tr>
									{/each}
									{#if libraryTracks.length === 0}
										<tr>
											<td colspan="6" class="empty-state">
												No tracks found. Scan a folder to add music.
											</td>
										</tr>
									{/if}
								</tbody>
							</table>
						</div>
					</div>
				</div>
			</div>

			<!-- Context Menu -->
			{#if contextMenuVisible && contextMenuTrack}
				<!-- svelte-ignore a11y_click_events_have_key_events -->
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div class="context-menu" style="left: {contextMenuX}px; top: {contextMenuPositionFromBottom ? 'auto' : contextMenuY + 'px'}; bottom: {contextMenuPositionFromBottom ? contextMenuY + 'px' : 'auto'};" onclick={(e) => e.stopPropagation()}>
					<div
						class="context-menu-item"
						onclick={() => {
							if (contextMenuTrack) {
								addToPlaylist(contextMenuTrack);
							}
							closeContextMenu();
						}}
					>
						Add to Play Queue
					</div>
					<div
						class="context-menu-item"
						onmouseenter={() => {
							if (submenuTimeout) clearTimeout(submenuTimeout);
							playlistSubmenuVisible = true;
							deviceSubmenuVisible = false;
						}}
						onmouseleave={() => {
							submenuTimeout = setTimeout(() => {
								playlistSubmenuVisible = false;
							}, 200);
						}}
					>
						Playlist
						<span class="submenu-arrow">▶</span>
						{#if playlistSubmenuVisible}
							<div
								class="context-menu-submenu {contextMenuPositionFromBottom ? 'positioned-from-bottom' : ''}"
								style="max-height: {submenuMaxHeight}px;"
								onmouseenter={() => {
									if (submenuTimeout) clearTimeout(submenuTimeout);
									playlistSubmenuVisible = true;
								}}
								onmouseleave={() => {
									submenuTimeout = setTimeout(() => {
										playlistSubmenuVisible = false;
									}, 200);
								}}
							>
								{#each playlists as playlist}
									<div class="context-menu-item">
										<input
											type="checkbox"
											bind:checked={trackPlaylistMembership[playlist.id]}
											onchange={(e) => toggleTrackInPlaylist(playlist.id, e.currentTarget.checked)}
											onclick={(e) => e.stopPropagation()}
											class="playlist-checkbox"
										/>
										<span
											onclick={(e) => {
												e.stopPropagation();
												const newState = !trackPlaylistMembership[playlist.id];
												trackPlaylistMembership[playlist.id] = newState;
												toggleTrackInPlaylist(playlist.id, newState);
											}}
											class="playlist-name"
										>
											{playlist.name}
										</span>
									</div>
								{/each}
								<div class="context-menu-divider"></div>
								<div class="context-menu-item new-playlist">
									<input
										type="text"
										placeholder="New playlist name..."
										bind:value={newPlaylistName}
										onkeydown={(e) => {
											if (e.key === "Enter") {
												contextMenuTrack && createPlaylistAndAddTrack();
											}
											e.stopPropagation();
										}}
										onclick={(e) => e.stopPropagation()}
										onmouseenter={() => {
											if (submenuTimeout) clearTimeout(submenuTimeout);
											playlistSubmenuVisible = true;
										}}
										class="new-playlist-input"
									/>
									<button
										onclick={(e) => {
											e.stopPropagation();
											contextMenuTrack && createPlaylistAndAddTrack();
										}}
										class="btn-checkmark"
									>
										✓
									</button>
								</div>
							</div>
						{/if}
					</div>
					<div
						class="context-menu-item"
						onmouseenter={() => {
							if (deviceSubmenuTimeout) clearTimeout(deviceSubmenuTimeout);
							deviceSubmenuVisible = true;
							playlistSubmenuVisible = false;
						}}
						onmouseleave={() => {
							deviceSubmenuTimeout = setTimeout(() => {
								deviceSubmenuVisible = false;
							}, 200);
						}}
					>
						Send to Device
						<span class="submenu-arrow">▶</span>
						{#if deviceSubmenuVisible}
							<div
								class="context-menu-submenu {contextMenuPositionFromBottom ? 'positioned-from-bottom' : ''}"
								onmouseenter={() => {
									if (deviceSubmenuTimeout) clearTimeout(deviceSubmenuTimeout);
									deviceSubmenuVisible = true;
								}}
								onmouseleave={() => {
									deviceSubmenuTimeout = setTimeout(() => {
										deviceSubmenuVisible = false;
									}, 200);
								}}
							>
								{#if availableDevices.length === 0}
									<div class="context-menu-item disabled">
										No available devices
									</div>
								{:else}
									{#each availableDevices as device}
										<div
											class="context-menu-item"
											onclick={() => {
												if (contextMenuTrack) {
													copyTrackToDevice(contextMenuTrack, device);
												}
												closeContextMenu();
											}}
										>
											<span class="device-icon">💾</span>
											<span class="device-name">{device.name}</span>
										</div>
									{/each}
								{/if}
							</div>
						{/if}
					</div>
					<div
						class="context-menu-item"
						onclick={() => {
							if (contextMenuTrack) {
								invoke("reveal_in_file_explorer", { filePath: contextMenuTrack.file_path });
							}
							closeContextMenu();
						}}
					>
						Reveal in File Explorer
					</div>
				</div>
			{/if}

			<!-- Playlist Context Menu -->
			{#if playlistContextMenuVisible && contextMenuPlaylist}
				<!-- svelte-ignore a11y_click_events_have_key_events -->
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div class="context-menu" style="left: {playlistContextMenuX}px; top: {playlistContextMenuPositionFromBottom ? 'auto' : playlistContextMenuY + 'px'}; bottom: {playlistContextMenuPositionFromBottom ? playlistContextMenuY + 'px' : 'auto'};" onclick={(e) => e.stopPropagation()}>
					<div
						class="context-menu-item"
						onclick={() => {
							if (contextMenuPlaylist) {
								initiateDeletePlaylist(contextMenuPlaylist);
							}
						}}
					>
						Delete Playlist
					</div>
				</div>
			{/if}

			<!-- Delete Playlist Confirmation Dialog -->
			{#if showDeleteDialog && playlistToDelete}
				<div class="dialog-overlay">
					<div class="dialog">
						<h3>Delete Playlist</h3>
						<p>
							Are you sure you want to delete the playlist "<strong>{playlistToDelete.name}</strong>"?
						</p>
						<p>
							This playlist contains {playlistTrackCount} song{playlistTrackCount !== 1 ? 's' : ''}.
						</p>
						<p class="dialog-warning">
							This will only remove the playlist from the database. The songs will not be deleted from your disk.
						</p>
						<div class="dialog-buttons">
							<button
								onclick={() => {
									showDeleteDialog = false;
									playlistToDelete = null;
								}}
								class="btn btn-secondary"
							>
								Cancel
							</button>
							<button
								onclick={confirmDeletePlaylist}
								class="btn btn-danger"
							>
								Delete
							</button>
						</div>
					</div>
				</div>
			{/if}

			{#if playlistPanelVisible}
				<div class="panel-right">
					<div class="app-header">
						<h1>Whiplash</h1>
						<div class="app-header-buttons">
							<button
								onclick={() => playlistPanelVisible = true}
								class="btn btn-icon btn-config"
								title="Toggle Playlist Panel"
							>
								<svg
									xmlns="http://www.w3.org/2000/svg"
									width="20"
									height="20"
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									stroke-width="2"
									stroke-linecap="round"
									stroke-linejoin="round"
								>
									<line x1="8" y1="6" x2="21" y2="6"></line>
									<line x1="8" y1="12" x2="21" y2="12"></line>
									<line x1="8" y1="18" x2="21" y2="18"></line>
									<line x1="3" y1="6" x2="3.01" y2="6"></line>
									<line x1="3" y1="12" x2="3.01" y2="12"></line>
									<line x1="3" y1="18" x2="3.01" y2="18"></line>
								</svg>
							</button>
							<button
								onclick={() => (showConfig = true)}
								class="btn btn-icon btn-config"
								title="Configuration"
							>
								<svg
									class="cog-icon"
									xmlns="http://www.w3.org/2000/svg"
									width="20"
									height="20"
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									stroke-width="2"
									stroke-linecap="round"
									stroke-linejoin="round"
								>
									<circle cx="12" cy="12" r="3"></circle>
									<path
										d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"
									></path>
								</svg>
							</button>
							<button
								onclick={() => invoke("open_devtools")}
								class="btn btn-icon btn-config"
								title="Open DevTools"
							>
								<svg
									class="dev-icon"
									xmlns="http://www.w3.org/2000/svg"
									width="20"
									height="20"
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									stroke-width="2"
									stroke-linecap="round"
									stroke-linejoin="round"
								>
									<polyline points="16 18 22 12 16 6"></polyline>
									<polyline points="8 6 2 12 8 18"></polyline>
								</svg>
							</button>
						</div>
					</div>
					<div class="playqueue-header">
						<h3>Play Queue</h3>
						<div class="playqueue-buttons">
							<button
								onclick={clearPlaylist}
								class="btn btn-small"
								disabled={currentPlaylist.length === 0}
							>
								Clear
							</button>
							<button
								onclick={() => playlistPanelVisible = false}
								class="btn btn-small btn-close"
							>
								×
							</button>
						</div>
					</div>
					<div class="playqueue-list">
						{#each currentPlaylist as track, index}
							<div
								class="track-item"
								class:active={currentTrack?.id === track.id}
								ondblclick={() => {
									console.log(
										"Double-clicked track:",
										track.title || track.file_name,
									);
									playTrack(track);
								}}
								onkeydown={(e) => {
									if (e.key === "Enter" || e.key === " ")
										playTrack(track);
								}}
								role="button"
								tabindex="0"
							>
								<div class="track-index">{index + 1}</div>
								<div class="track-info">
									<div class="track-title">
										{track.title || track.file_name}
									</div>
									<div class="track-artist">
										{track.artist || "Unknown"}
									</div>
								</div>
								<div class="track-duration">
									{formatDuration(track.duration)}
								</div>
								<button
									onclick={(e) => {
										e.stopPropagation();
										removeFromPlaylist(index);
									}}
									class="btn btn-small btn-remove"
									title="Remove from playlist"
								>
									×
								</button>
							</div>
						{/each}
						{#if currentPlaylist.length === 0}
							<div class="empty-state">
								Playlist is empty. Add tracks from the library.
							</div>
						{/if}
					</div>
				</div>
			{/if}
		</div>
	{:else}
		<div class="welcome-screen">
			<h2>Welcome to Whiplash</h2>
			<p style="margin-bottom: 20px;">
				Initialize the database to get started with your music library.
			</p>
			<button class="btn btn-primary" onclick={initializeDatabase}>
				Choose Folder & Initialize Database
			</button>
		</div>
	{/if}
</div>

<ConfigDialog bind:show={showConfig} onFoldersChanged={loadTracks} />

<style>
	:global(body) {
		margin: 0;
		padding: 0;
		font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto,
			sans-serif;
		background: #1a1a1a;
		color: #e0e0e0;
		user-select: none;
		-webkit-user-select: none;
	}

	:global(input, textarea) {
		user-select: auto;
		-webkit-user-select: auto;
	}

	* {
		margin: 0;
		padding: 0;
		box-sizing: border-box;
	}

	:global(::-webkit-scrollbar) {
		width: 10px;
		height: 10px;
	}

	:global(::-webkit-scrollbar-track) {
		background: #1a1a1a;
	}

	:global(::-webkit-scrollbar-thumb) {
		background: #2d2d2d;
		border-radius: 5px;
	}

	:global(::-webkit-scrollbar-thumb:hover) {
		background: #3e3e3e;
	}

	.app {
		height: 100vh;
		display: flex;
		flex-direction: column;
	}

	.app-header {
		background: #121212;
		padding: 12px 16px;
		display: flex;
		justify-content: space-between;
		align-items: center;
		border-bottom: 1px solid #2d2d2d;
	}

	.app-header h1 {
		margin: 0;
		font-size: 18px;
		color: #ffffff;
	}

	.app-header-buttons {
		display: flex;
		gap: 8px;
		align-items: center;
	}

	.btn-config {
		background: transparent;
		color: #a0a0a0;
		padding: 8px;
		border-radius: 50%;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: all 0.2s;
	}

	.btn-config:hover {
		background: #2d2d2d;
		color: #e0e0e0;
	}

	.cog-icon {
		transition: transform 0.4s ease-out;
	}

	.btn-config:hover .cog-icon {
		transform: rotate(90deg);
	}

	.main-content {
		flex: 1;
		display: flex;
		overflow: hidden;
	}

	.library-column {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		transition: flex 0.3s ease;
	}

	.panel-right {
		flex: 0 0 350px;
		background: #121212;
		border-left: 1px solid #2d2d2d;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		min-width: 0;
	}

	.player-area {
		height: 15vh;
		min-height: 150px;
		background: #121212;
		border-bottom: 1px solid #2d2d2d;
		display: flex;
		flex-direction: row;
	}

	.player-left-column {
		flex: 0 0 80%;
		display: flex;
		flex-direction: column;
		justify-content: center;
	}

	.player-right-column {
		flex: 0 0 20%;
		max-width: 20%;
		min-width: 0;
		display: flex;
		flex-direction: column;
		border-left: 1px solid #2d2d2d;
	}

	.visualizer-section {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		overflow: hidden;
		min-width: 0;
	}

	.player-buttons-section {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		border-top: 1px solid #2d2d2d;
	}

	.current-song-section {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 16px;
	}

	.song-info-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
	}

	.song-name {
		font-size: 14px;
		font-weight: 500;
		color: #e0e0e0;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		max-width: 70%;
	}

	.song-time {
		font-size: 12px;
		color: #a0a0a0;
		font-variant-numeric: tabular-nums;
	}

	.progress-row {
		display: flex;
		align-items: center;
	}

	.progress-slider {
		flex: 1;
		-webkit-appearance: none;
		appearance: none;
		height: 4px;
		border-radius: 2px;
		background: #2d2d2d;
	}

	.progress-slider::-webkit-slider-thumb {
		-webkit-appearance: none;
		appearance: none;
		width: 12px;
		height: 12px;
		border-radius: 50%;
		background: #808080;
		cursor: pointer;
	}

	.controls-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 16px;
	}

	.playback-buttons {
		display: flex;
		gap: 8px;
	}

	.volume-section {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.volume-slider-compact {
		width: 80px;
		-webkit-appearance: none;
		appearance: none;
		height: 4px;
		border-radius: 2px;
		background: #2d2d2d;
	}

	.volume-slider-compact::-webkit-slider-thumb {
		-webkit-appearance: none;
		appearance: none;
		width: 12px;
		height: 12px;
		border-radius: 50%;
		background: #ffffff;
		cursor: pointer;
	}

	.btn-icon {
		background: transparent;
		border: none;
		color: #e0e0e0;
		cursor: pointer;
		padding: 4px;
		font-size: 18px;
	}

	.btn-icon:hover {
		color: #ffffff;
	}

	.library-section {
		flex: 1;
		display: flex;
		overflow: hidden;
	}

	.library-left {
		flex: 0 0 15%;
		display: flex;
		flex-direction: column;
		border-right: 1px solid #2d2d2d;
		background: #121212;
		overflow-y: auto;
	}

	.library-right {
		flex: 0 0 85%;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.filter-section,
	.playlist-section,
	.devices-section {
		padding: 6px;
		border-bottom: 1px solid #2d2d2d;
	}

	.section-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin: 0 0 2px 0;
		font-size: 12px;
		color: #a0a0a0;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		cursor: pointer;
		user-select: none;
	}

	.section-header:hover {
		color: #e0e0e0;
	}

	.section-header-actions {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.btn-add-playlist {
		background: transparent;
		border: none;
		color: #a0a0a0;
		font-size: 16px;
		cursor: pointer;
		padding: 2px 6px;
		line-height: 1;
	}

	.btn-add-playlist:hover {
		color: #e0e0e0;
	}

	.chevron {
		transition: transform 0.2s ease;
		font-size: 10px;
	}

	.chevron.collapsed {
		transform: rotate(-90deg);
	}

	.inline-playlist-input {
		display: flex;
		gap: 4px;
		margin-bottom: 8px;
		width: 100%;
		box-sizing: border-box;
	}

	.inline-playlist-input input {
		flex: 1;
		min-width: 0;
		padding: 6px 8px;
		border-radius: 4px;
		border: 1px solid #2d2d2d;
		background: #1a1a1a;
		color: #e0e0e0;
		font-size: 12px;
		box-sizing: border-box;
	}

	.inline-playlist-input input.error {
		border-color: #ff4444;
		animation: blink-error 1s ease-in-out;
	}

	.inline-playlist-input .btn-check,
	.inline-playlist-input .btn-cancel {
		background: transparent;
		border: none;
		color: #a0a0a0;
		cursor: pointer;
		padding: 4px 6px;
		font-size: 12px;
	}

	.inline-playlist-input .btn-check:hover {
		color: #4caf50;
	}

	.inline-playlist-input .btn-cancel:hover {
		color: #ff4444;
	}

	.filter-list.collapsed {
		display: none;
	}

	.filter-list {
		display: flex;
		flex-direction: column;
		gap: 1px;
	}

	.filter-item {
		padding: 2px 4px;
		border-radius: 6px;
		cursor: pointer;
		transition: background 0.2s;
		font-size: 10px;
		color: #e0e0e0;
		user-select: none;
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.device-icon {
		font-size: 12px;
	}

	.device-name {
		flex: 1;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.filter-item:hover {
		background: #2d2d2d;
	}

	.filter-item.selected {
		background: #3e3e3e;
		color: #ffffff;
		font-weight: 500;
	}

	.library-header {
		padding: 16px;
		border-bottom: 1px solid #2d2d2d;
		display: flex;
		gap: 12px;
		align-items: center;
		position: relative;
	}

	.search-box {
		flex: 1;
	}

	.playlist-dropdown {
		position: absolute;
		right: 16px;
		top: 100%;
		margin-top: 8px;
		background: #2d2d2d;
		border: 1px solid #3e3e3e;
		border-radius: 8px;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
		z-index: 1000;
		min-width: 200px;
		padding: 0;
	}

	.playlist-dropdown-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 12px;
		border-bottom: 1px solid #3e3e3e;
		font-size: 13px;
		font-weight: 500;
		color: #e0e0e0;
	}

	.btn-close {
		background: transparent;
		border: none;
		color: #a0a0a0;
		cursor: pointer;
		padding: 2px 6px;
		font-size: 14px;
		line-height: 1;
	}

	.btn-close:hover {
		color: #e0e0e0;
	}

	.playlist-dropdown-list {
		max-height: 200px;
		overflow-y: auto;
		padding: 4px;
	}

	.playlist-dropdown-item {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 12px;
		border-radius: 4px;
		cursor: default;
	}

	.playlist-dropdown-item:hover {
		background: #3e3e3e;
	}

	.playlist-dropdown-footer {
		padding: 8px;
		border-top: 1px solid #3e3e3e;
	}

	.btn-small {
		padding: 4px 8px;
		font-size: 12px;
		min-width: 24px;
	}

	.new-playlist-input.error {
		border-color: #ff4444 !important;
		animation: blink-red 0.5s ease-in-out 3;
	}

	@keyframes blink-red {
		0%, 100% {
			border-color: #ff4444;
		}
		50% {
			border-color: #2d2d2d;
		}
	}

	.search-box {
		display: flex;
		position: relative;
	}

	.search-input {
		width: 100%;
		padding: 10px 14px;
		padding-right: 36px;
		border-radius: 8px;
		border: 1px solid #2d2d2d;
		background: #1a1a1a;
		color: #e0e0e0;
		font-size: 14px;
	}

	.search-input:focus {
		outline: none;
		border-color: #ffffff;
	}

	.btn-clear-search {
		position: absolute;
		right: 8px;
		top: 50%;
		transform: translateY(-50%);
		background: transparent;
		border: none;
		color: #a0a0a0;
		cursor: pointer;
		padding: 4px 6px;
		font-size: 14px;
		line-height: 1;
	}

	.btn-clear-search:hover {
		color: #e0e0e0;
	}

	.track-table-container {
		flex: 1;
		overflow-y: auto;
	}

	.track-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 14px;
		table-layout: fixed;
	}

	.track-table thead {
		position: sticky;
		top: 0;
		background: #121212;
		z-index: 1;
	}

	.track-table th {
		padding: 8px 8px;
		text-align: left;
		font-weight: 500;
		color: #a0a0a0;
		border-bottom: 1px solid #2d2d2d;
		user-select: none;
	}

	.track-table th.sortable {
		cursor: pointer;
		transition: color 0.2s;
	}

	.track-table th.sortable:hover {
		color: #e0e0e0;
	}

	.track-table th.sorted {
		color: #ffffff;
	}

	.track-table tbody tr {
		border-bottom: 1px solid #1e1e1e;
		cursor: pointer;
		transition: background 0.2s;
	}

	.track-table tbody tr:hover {
		background: #2d2d2d;
	}

	.track-table tbody tr.active {
		background: #3e3e3e;
	}

	.track-table td {
		padding: 6px 16px;
		color: #e0e0e0;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.track-table td.track-title-cell {
		font-weight: 500;
		color: #ffffff;
		max-width: 300px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.track-list {
		flex: 1;
		overflow-y: auto;
		padding: 8px;
	}

	.context-menu {
		position: fixed;
		background: #2d2d2d;
		border: 1px solid #3e3e3e;
		border-radius: 8px;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
		z-index: 1000;
		min-width: 180px;
		padding: 4px;
	}

	.context-menu-item {
		padding: 8px 12px;
		border-radius: 4px;
		cursor: pointer;
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 13px;
		color: #e0e0e0;
		transition: background 0.2s;
		position: relative;
	}

	.playlist-checkbox {
		margin: 0;
		cursor: pointer;
	}

	.playlist-name {
		flex: 1;
		cursor: pointer;
	}

	.context-menu-item:hover {
		background: #3e3e3e;
	}

	.context-menu-item.disabled {
		color: #666666;
		cursor: not-allowed;
	}

	.context-menu-item.disabled:hover {
		background: transparent;
	}

	.context-menu-submenu .device-icon {
		font-size: 14px;
		margin-right: 8px;
	}

	.context-menu-submenu .device-name {
		flex: 1;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.context-menu-submenu {
		position: absolute;
		left: 100%;
		top: 0;
		margin-left: -4px;
		background: #2d2d2d;
		border: 1px solid #3e3e3e;
		border-radius: 8px;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
		min-width: 180px;
		max-height: 400px;
		overflow-y: auto;
		padding: 4px;
	}

	.context-menu-submenu.positioned-from-bottom {
		top: auto;
		bottom: 0;
	}

	.context-menu-divider {
		height: 1px;
		background: #3e3e3e;
		margin: 4px 0;
	}

	.submenu-arrow {
		color: #a0a0a0;
		font-size: 10px;
	}

	.new-playlist {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 4px;
	}

	.new-playlist-input {
		flex: 1;
		background: #1a1a1a;
		border: 1px solid #3e3e3e;
		border-radius: 4px;
		padding: 4px 8px;
		color: #e0e0e0;
		font-size: 12px;
		outline: none;
	}

	.new-playlist-input:focus {
		border-color: #ffffff;
	}

	.btn-checkmark {
		background: #3e3e3e;
		border: none;
		border-radius: 4px;
		padding: 4px 8px;
		color: #ffffff;
		cursor: pointer;
		font-size: 12px;
		transition: background 0.2s;
	}

	.btn-checkmark:hover {
		background: #4e4e4e;
	}

	.track-item {
		padding: 4px;
		border-radius: 8px;
		cursor: pointer;
		transition: background 0.2s;
		display: flex;
		justify-content: space-between;
		align-items: center;
	}

	.track-item:hover {
		background: #2d2d2d;
	}

	.track-item.active {
		background: #3e3e3e;
		border: 1px solid #ffffff;
	}

	.track-info {
		flex: 1;
		min-width: 0;
	}

	.track-title {
		font-weight: 500;
		color: #e0e0e0;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.track-artist {
		font-size: 12px;
		color: #a0a0a0;
		margin-top: 4px;
	}

	.track-duration {
		font-size: 12px;
		color: #a0a0a0;
		margin-left: 12px;
		margin-right: 6px;
	}

	.track-index {
		font-size: 12px;
		color: #a0a0a0;
		margin-right: 12px;
		min-width: 24px;
		text-align: center;
	}

	.playqueue-header {
		background: #1a1a1a;
		padding: 12px 16px;
		border-bottom: 1px solid #2d2d2d;
		display: flex;
		justify-content: space-between;
		align-items: center;
	}

	.playqueue-header h3 {
		margin: 0;
		font-size: 14px;
		color: #e0e0e0;
	}

	.playqueue-buttons {
		display: flex;
		gap: 8px;
	}

	.btn-close {
		background: #2d2d2d;
		color: #e0e0e0;
		border: none;
		border-radius: 4px;
		padding: 4px 8px;
		font-size: 16px;
		line-height: 1;
	}

	.btn-close:hover {
		background: #3e3e3e;
	}

	.playqueue-list {
		flex: 1;
		overflow-y: auto;
		padding: 8px;
	}

	.btn-add {
		background: #383838;
		color: white;
		border: none;
		padding: 4px 8px;
		border-radius: 4px;
		cursor: pointer;
		font-size: 16px;
	}

	.btn-add:hover {
		background: #575757;
	}

	.btn-remove {
		background: #383838;
		color: white;
		border: none;
		padding: 4px 8px;
		border-radius: 4px;
		cursor: pointer;
		font-size: 16px;
	}

	.btn-remove:hover {
		background: #999999;
	}

	.empty-state {
		padding: 32px;
		text-align: center;
		color: #a0a0a0;
	}

	.btn {
		padding: 10px 20px;
		border-radius: 8px;
		border: none;
		font-size: 14px;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.2s;
	}

	.btn-small {
		padding: 6px 12px;
		font-size: 12px;
		background: #383838;
		color: #e0e0e0;
	}

	.btn-small:hover:not(:disabled) {
		background: #525252;
	}

	.btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.btn-primary {
		background: #454545;
		color: white;
	}

	.btn-primary:hover {
		background: #525252;
	}

	.btn-secondary {
		background: #3e3e3e;
		color: #e0e0e0;
	}

	.btn-secondary:hover {
		background: #3e3e3e;
	}

	.btn-danger {
		background: #dc3545;
		color: white;
	}

	.btn-danger:hover {
		background: #c82333;
	}

	.btn-control {
		background: #3e3e3e;
		color: #e0e0e0;
		width: 48px;
		height: 48px;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 20px;
	}

	.btn-control:hover {
		background: #585b70;
	}

	.btn-large {
		width: 64px;
		height: 64px;
		font-size: 28px;
	}

	.btn-small {
		padding: 6px 12px;
		font-size: 12px;
	}

	.welcome-screen {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		padding: 24px;
	}

	.welcome-screen h2 {
		font-size: 32px;
		color: #ffffff;
		margin-bottom: 16px;
	}

	.welcome-screen p {
		font-size: 16px;
		color: #a0a0a0;
	}

	.dialog-overlay {
		position: fixed;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0;
		background: rgba(0, 0, 0, 0.7);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 1000;
	}

	.dialog {
		background: #2d2d2d;
		border-radius: 8px;
		padding: 24px;
		max-width: 400px;
		width: 90%;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
	}

	.dialog h3 {
		margin: 0 0 16px 0;
		color: #e0e0e0;
		font-size: 18px;
	}

	.dialog p {
		margin: 0 0 12px 0;
		color: #e0e0e0;
		line-height: 1.5;
	}

	.dialog-warning {
		color: #ffc107;
		font-size: 13px;
		margin-top: 16px;
		padding: 8px 12px;
		background: rgba(255, 193, 7, 0.1);
		border-radius: 4px;
	}

	.dialog-buttons {
		display: flex;
		gap: 12px;
		margin-top: 24px;
		justify-content: flex-end;
	}
</style>
