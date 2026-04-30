<script lang="ts">
	import { invoke } from "@tauri-apps/api/core";
	import { open } from "@tauri-apps/plugin-dialog";
	import type { WatchFolder, Device, Drive, Folder } from "$lib/types";

	let { show = $bindable(false), onFoldersChanged } = $props<{
		show: boolean;
		onFoldersChanged?: () => void;
	}>();

	let activeCategory = $state("watch_folders");
	let watchFolders = $state<WatchFolder[]>([]);
	let scanningFolders = $state<number[]>([]);

	// Device management state
	let devices = $state<Device[]>([]);
	let drives = $state<Drive[]>([]);
	let selectedDrive = $state<Drive | null>(null);
	let driveFolders = $state<Folder[]>([]);
	let loadingDrives = $state(false);
	let loadingFolders = $state(false);

	// Load watch folders from database when dialog opens
	$effect(() => {
		if (show) {
			loadWatchFolders();
			loadDevices();
		}
	});

	async function loadWatchFolders() {
		try {
			watchFolders = await invoke<WatchFolder[]>("get_watch_folders");
		} catch (error) {
			console.error("Failed to load watch folders:", error);
		}
	}

	async function addWatchFolder() {
		try {
			const selected = await open({
				directory: true,
				multiple: false,
				title: "Select Watch Folder",
			});

			if (selected) {
				const newFolder = await invoke<WatchFolder>(
					"add_watch_folder",
					{
						path: selected,
					},
				);
				watchFolders = [...watchFolders, newFolder];

				scanFolderProcess(newFolder);
			}
		} catch (error) {
			console.error("Failed to add watch folder:", error);
		}
	}

	async function scanFolderProcess(folder: WatchFolder) {
		scanningFolders = [...scanningFolders, folder.id];
		try {
			await invoke("scan_folder", { folderPath: folder.path });
			if (onFoldersChanged) {
				onFoldersChanged();
			}
		} catch (error) {
			console.error("Failed to scan folder:", error);
		} finally {
			scanningFolders = scanningFolders.filter((id) => id !== folder.id);
		}
	}

	async function toggleWatchFolder(folder: WatchFolder) {
		try {
			const newDisabledState = !folder.is_disabled;
			await invoke("toggle_watch_folder", {
				id: folder.id,
				isDisabled: newDisabledState,
			});
			// Update local state
			watchFolders = watchFolders.map((f) =>
				f.id === folder.id
					? { ...f, is_disabled: newDisabledState }
					: f,
			);
			if (onFoldersChanged) {
				onFoldersChanged();
			}
		} catch (error) {
			console.error("Failed to toggle watch folder:", error);
		}
	}

	async function removeWatchFolder(id: number) {
		try {
			await invoke("remove_watch_folder", { id });
			watchFolders = watchFolders.filter((f) => f.id !== id);
			if (onFoldersChanged) {
				onFoldersChanged();
			}
		} catch (error) {
			console.error("Failed to remove watch folder:", error);
		}
	}

	// Device management functions
	async function loadDevices() {
		try {
			devices = await invoke<Device[]>("get_devices");
		} catch (error) {
			console.error("Failed to load devices:", error);
		}
	}

	async function loadDrives() {
		try {
			loadingDrives = true;
			drives = await invoke<Drive[]>("get_drives");
		} catch (error) {
			console.error("Failed to load drives:", error);
		} finally {
			loadingDrives = false;
		}
	}

	async function selectDrive(drive: Drive) {
		selectedDrive = drive;
		try {
			loadingFolders = true;
			driveFolders = await invoke<Folder[]>("get_drive_folders", { drivePath: drive.letter });
		} catch (error) {
			console.error("Failed to load drive folders:", error);
			driveFolders = [];
		} finally {
			loadingFolders = false;
		}
	}

	async function addDevice(folder: Folder) {
		try {
			const driveName = selectedDrive?.name || selectedDrive?.letter || "";
			const deviceName = driveName ? `${driveName} - ${folder.name}` : folder.name;
			await invoke("add_device", { path: folder.path, name: deviceName });
			await loadDevices();
		} catch (error) {
			console.error("Failed to add device:", error);
		}
	}

	async function removeDevice(id: number) {
		try {
			await invoke("remove_device", { id });
			await loadDevices();
		} catch (error) {
			console.error("Failed to remove device:", error);
		}
	}

	function closeDialog() {
		show = false;
	}
</script>

{#if show}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="modal-overlay" onclick={closeDialog}>
		<div class="modal-content" onclick={(e) => e.stopPropagation()}>
			<div class="modal-sidebar">
				<h3>Configuration</h3>
				<ul class="category-list">
					<!-- svelte-ignore a11y_click_events_have_key_events -->
					<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
					<li
						class:active={activeCategory === "watch_folders"}
						onclick={() => (activeCategory = "watch_folders")}
					>
						Watch Folders
					</li>
					<!-- svelte-ignore a11y_click_events_have_key_events -->
					<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
					<li
						class:active={activeCategory === "devices"}
						onclick={() => {
							activeCategory = "devices";
							if (drives.length === 0) {
								loadDrives();
							}
						}}
					>
						Devices
					</li>
				</ul>
			</div>

			<div class="modal-main">
				<div class="modal-header">
					<h2>
						{#if activeCategory === "watch_folders"}
							Watch Folders
						{:else if activeCategory === "devices"}
							Devices
						{/if}
					</h2>
					<button class="btn-close" onclick={closeDialog}>✕</button>
				</div>

				<div class="modal-body">
					{#if activeCategory === "watch_folders"}
						<div class="watch-folders-panel">
							<p class="description">
								Add folders to your library. We will scan these
								folders for new music.
							</p>

							<button
								class="btn btn-primary"
								onclick={addWatchFolder}
							>
								+ Add Folder
							</button>

							<div class="folders-list">
								{#each watchFolders as folder}
									<div class="folder-item">
										<div
											class="folder-path"
											class:disabled={folder.is_disabled}
										>
											{folder.path}
										</div>
										<div class="folder-action-status">
											{#if scanningFolders.includes(folder.id)}
												<span
													class="loading-icon"
													title="Scanning..."
												>
													<svg
														xmlns="http://www.w3.org/2000/svg"
														width="16"
														height="16"
														viewBox="0 0 24 24"
														fill="none"
														stroke="currentColor"
														stroke-width="2"
														stroke-linecap="round"
														stroke-linejoin="round"
														class="spin"
													>
														<line
															x1="12"
															y1="2"
															x2="12"
															y2="6"
														></line>
														<line
															x1="12"
															y1="18"
															x2="12"
															y2="22"
														></line>
														<line
															x1="4.93"
															y1="4.93"
															x2="7.76"
															y2="7.76"
														></line>
														<line
															x1="16.24"
															y1="16.24"
															x2="19.07"
															y2="19.07"
														></line>
														<line
															x1="2"
															y1="12"
															x2="6"
															y2="12"
														></line>
														<line
															x1="18"
															y1="12"
															x2="22"
															y2="12"
														></line>
														<line
															x1="4.93"
															y1="19.07"
															x2="7.76"
															y2="16.24"
														></line>
														<line
															x1="16.24"
															y1="7.76"
															x2="19.07"
															y2="4.93"
														></line>
													</svg>
												</span>
											{/if}
											<label class="toggle-label">
												<input
													type="checkbox"
													checked={!folder.is_disabled}
													onchange={() =>
														toggleWatchFolder(
															folder,
														)}
												/>
												Enabled
											</label>
											<button
												class="btn btn-danger"
												onclick={() =>
													removeWatchFolder(
														folder.id,
													)}
											>
												Remove
											</button>
										</div>
									</div>
								{/each}
								{#if watchFolders.length === 0}
									<div class="empty-state">
										No folders configured yet.
									</div>
								{/if}
							</div>
						</div>
					{:else if activeCategory === "devices"}
						<div class="devices-panel">
							<div class="devices-layout">
								<!-- Left column: Drives and folders -->
								<div class="devices-browse">
									<h3>Browse Drives</h3>
									{#if loadingDrives}
										<p class="loading">Loading drives...</p>
									{:else if drives.length === 0}
										<p class="empty">No drives found.</p>
									{:else}
										<div class="drives-list">
											{#each drives as drive}
												<!-- svelte-ignore a11y_click_events_have_key_events -->
												<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
												<div
													class="drive-item"
													class:selected={selectedDrive?.letter === drive.letter}
													onclick={() => selectDrive(drive)}
												>
													<span class="drive-icon">💾</span>
													<span class="drive-name">{drive.letter} {drive.name || ''}</span>
												</div>
											{/each}
										</div>
									{/if}

									{#if selectedDrive}
										<div class="folders-section">
											<h4>{selectedDrive.letter}</h4>
											{#if loadingFolders}
												<p class="loading">Loading folders...</p>
											{:else if driveFolders.length === 0}
												<p class="empty">No folders found.</p>
											{:else}
												<div class="folders-list">
													{#each driveFolders as folder}
													<div class="folder-item">
														<span class="folder-icon">📁</span>
														<span class="folder-name">{folder.name}</span>
														<button
															class="btn btn-add-device"
															onclick={() => addDevice(folder)}
															title="Add to devices"
														>
															+
														</button>
													</div>
												{/each}
												</div>
											{/if}
										</div>
									{/if}
								</div>

								<!-- Right column: Configured devices -->
								<div class="devices-configured">
									<h3>Configured Devices</h3>
									{#if devices.length === 0}
										<p class="empty">No devices configured yet.</p>
									{:else}
										<div class="devices-list">
											{#each devices as device}
											<div class="device-item">
												<span class="device-icon">💾</span>
												<div class="device-info">
													<div class="device-name">{device.name}</div>
													<div class="device-path">{device.path}</div>
												</div>
												<button
													class="btn btn-remove-device"
													onclick={() => removeDevice(device.id)}
													title="Remove device"
												>
													×
												</button>
											</div>
											{/each}
										</div>
									{/if}
								</div>
							</div>
						</div>
					{/if}
				</div>
			</div>
		</div>
	</div>
{/if}

<style>
	.modal-overlay {
		position: fixed;
		top: 0;
		left: 0;
		width: 100vw;
		height: 100vh;
		background: rgba(0, 0, 0, 0.6);
		backdrop-filter: blur(4px);
		z-index: 1000;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.modal-content {
		width: 80%;
		height: 80%;
		background: #1a1a1a;
		border-radius: 12px;
		display: flex;
		flex-direction: row;
		overflow: hidden;
		box-shadow: 0 20px 40px rgba(0, 0, 0, 0.4);
		border: 1px solid #2d2d2d;
	}

	.modal-sidebar {
		width: 15%;
		min-width: 180px;
		background: #121212;
		border-right: 1px solid #2d2d2d;
		padding: 24px 0;
	}

	.modal-sidebar h3 {
		color: #a0a0a0;
		font-size: 14px;
		text-transform: uppercase;
		letter-spacing: 1px;
		margin-bottom: 16px;
		padding: 0 24px;
	}

	.category-list {
		list-style: none;
		padding: 0;
		margin: 0;
	}

	.category-list li {
		padding: 12px 24px;
		color: #e0e0e0;
		cursor: pointer;
		transition:
			background 0.2s,
			color 0.2s;
	}

	.category-list li:hover {
		background: #2d2d2d;
	}

	.category-list li.active {
		background: rgba(255, 255, 255, 0.15);
		color: #ffffff;
		border-right: 3px solid #ffffff;
	}

	.modal-main {
		flex: 1;
		display: flex;
		flex-direction: column;
		background: #1a1a1a;
	}

	.modal-header {
		padding: 24px;
		border-bottom: 1px solid #2d2d2d;
		display: flex;
		justify-content: space-between;
		align-items: center;
	}

	.modal-header h2 {
		margin: 0;
		color: #e0e0e0;
		font-size: 20px;
	}

	.btn-close {
		background: transparent;
		border: none;
		color: #a0a0a0;
		font-size: 24px;
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 32px;
		height: 32px;
		border-radius: 50%;
		transition:
			background 0.2s,
			color 0.2s;
	}

	.btn-close:hover {
		background: #2d2d2d;
		color: #ffffff;
	}

	.modal-body {
		flex: 1;
		padding: 24px;
		display: flex;
		flex-direction: column;
		min-height: 0;
	}

	.description {
		color: #a0a0a0;
		margin-bottom: 24px;
	}

	.btn {
		padding: 8px 16px;
		border: none;
		border-radius: 6px;
		cursor: pointer;
		font-weight: 500;
		transition: all 0.2s;
		font-size: 14px;
	}

	.btn-primary {
		background: #454545;
		color: #e0e0e0;
	}

	.btn-primary:hover {
		background: #525252;
	}

	.btn-danger {
		background: rgba(255, 255, 255, 0.1);
		color: #ffffff;
		border: 1px solid rgba(255, 255, 255, 0.2);
	}

	.btn-danger:hover {
		background: #ffffff;
		color: #11111b;
	}

	.watch-folders-panel {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-height: 0;
	}

	.folders-list {
		margin-top: 24px;
		display: flex;
		flex-direction: column;
		gap: 12px;
		flex: 1;
		overflow-y: auto;
		min-height: 0;
		padding-right: 8px; /* Give scrollbar some breathing room */
	}

	.folder-item {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 16px;
		background: #121212;
		border-radius: 8px;
		border: 1px solid #2d2d2d;
	}

	.folder-path {
		word-break: break-all;
		padding-right: 16px;
		color: #e0e0e0;
		transition: color 0.2s;
	}

	.folder-path.disabled {
		color: #666666;
		text-decoration: line-through;
	}

	.folder-action-status {
		display: flex;
		align-items: center;
		gap: 16px;
	}

	.loading-icon {
		color: #ffffff;
		display: flex;
		align-items: center;
		padding: 4px;
	}

	.spin {
		animation: spin 1.5s linear infinite;
	}

	@keyframes spin {
		0% {
			transform: rotate(0deg);
		}
		100% {
			transform: rotate(360deg);
		}
	}

	.folder-actions {
		display: flex;
		align-items: center;
		gap: 16px;
	}

	.toggle-label {
		display: flex;
		align-items: center;
		gap: 8px;
		color: #a0a0a0;
		cursor: pointer;
		font-size: 14px;
	}

	.toggle-label input[type="checkbox"] {
		accent-color: #ffffff;
		width: 16px;
		height: 16px;
		cursor: pointer;
	}

	.empty-state {
		text-align: center;
		padding: 40px;
		color: #666666;
		background: #121212;
		border-radius: 8px;
		border: 1px dashed #2d2d2d;
	}

	.devices-panel {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-height: 0;
	}

	.devices-layout {
		display: flex;
		gap: 24px;
		height: 100%;
		min-height: 0;
	}

	.devices-browse {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 16px;
		min-height: 0;
		overflow: hidden;
	}

	.devices-configured {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 16px;
		min-height: 0;
		overflow: hidden;
	}

	.devices-browse h3,
	.devices-configured h3 {
		color: #e0e0e0;
		font-size: 16px;
		margin: 0;
	}

	.drives-list {
		display: flex;
		flex-direction: column;
		gap: 8px;
		overflow-y: auto;
		max-height: 300px;
	}

	.drive-item {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 12px;
		background: #121212;
		border-radius: 8px;
		border: 1px solid #2d2d2d;
		cursor: pointer;
		transition: all 0.2s;
	}

	.drive-item:hover {
		background: #1e1e1e;
	}

	.drive-item.selected {
		background: rgba(255, 255, 255, 0.1);
		border-color: #ffffff;
	}

	.drive-icon {
		font-size: 18px;
	}

	.drive-name {
		color: #e0e0e0;
		font-size: 14px;
	}

	.folders-section {
		display: flex;
		flex-direction: column;
		gap: 12px;
		overflow: hidden;
	}

	.folders-section h4 {
		color: #a0a0a0;
		font-size: 14px;
		margin: 0;
	}

	.folders-list {
		display: flex;
		flex-direction: column;
		gap: 8px;
		overflow-y: auto;
		max-height: 300px;
	}

	.folder-item {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 12px;
		background: #121212;
		border-radius: 6px;
		border: 1px solid #2d2d2d;
	}

	.folder-icon {
		font-size: 16px;
	}

	.folder-name {
		flex: 1;
		color: #e0e0e0;
		font-size: 13px;
	}

	.btn-add-device {
		background: #454545;
		color: #e0e0e0;
		border: none;
		border-radius: 4px;
		padding: 4px 8px;
		font-size: 16px;
		line-height: 1;
		cursor: pointer;
		transition: all 0.2s;
		min-width: 28px;
	}

	.btn-add-device:hover {
		background: #525252;
	}

	.devices-list {
		display: flex;
		flex-direction: column;
		gap: 8px;
		overflow-y: auto;
	}

	.device-item {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px;
		background: #121212;
		border-radius: 8px;
		border: 1px solid #2d2d2d;
	}

	.device-icon {
		font-size: 18px;
	}

	.device-info {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 4px;
		overflow: hidden;
	}

	.device-name {
		color: #e0e0e0;
		font-size: 14px;
		font-weight: 500;
	}

	.device-path {
		color: #666666;
		font-size: 12px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.btn-remove-device {
		background: rgba(255, 255, 255, 0.1);
		color: #ffffff;
		border: none;
		border-radius: 4px;
		padding: 4px 8px;
		font-size: 16px;
		line-height: 1;
		cursor: pointer;
		transition: all 0.2s;
		min-width: 28px;
	}

	.btn-remove-device:hover {
		background: #ffffff;
		color: #11111b;
	}

	.loading,
	.empty {
		color: #666666;
		font-size: 14px;
		padding: 20px;
		text-align: center;
	}
</style>
