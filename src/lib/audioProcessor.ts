import { convertFileSrc } from '@tauri-apps/api/core';

export class AudioProcessor {
	private audioContext: AudioContext | null = null;
	private sourceNode: MediaElementAudioSourceNode | null = null;
	private analyserNode: AnalyserNode | null = null;
	private compressorNode: DynamicsCompressorNode | null = null;
	private preGainNode: GainNode | null = null;
	private gainNode: GainNode | null = null;
	private equalizerBands: BiquadFilterNode[] = [];
	private audioElement: HTMLAudioElement | null = null;

	// Equalizer bands (frequencies in Hz)
	private readonly frequencies = [32, 64, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];

	constructor() {
		this.initializeAudioContext();
		// Initialize gainNode immediately so volume control works even before audio is loaded
		if (this.audioContext) {
			this.gainNode = this.audioContext.createGain();
			// Connect gainNode to destination so volume changes work immediately
			this.gainNode.connect(this.audioContext.destination);
		}
	}

	private initializeAudioContext() {
		this.audioContext = new (window.AudioContext || (window as any).webkitAudioContext)();
		this.setupEqualizer();
	}

	private setupEqualizer() {
		if (!this.audioContext) return;

		this.analyserNode = this.audioContext.createAnalyser();
		this.analyserNode.fftSize = 2048;
		this.analyserNode.smoothingTimeConstant = 0.8;

		// Add a pre-gain to forcefully boost quiet tracks
		this.preGainNode = this.audioContext.createGain();
		this.preGainNode.gain.value = 5.0; // Boost signal (+9.5dB)

		// Add a compressor for loudness normalization
		this.compressorNode = this.audioContext.createDynamicsCompressor();
		this.compressorNode.threshold.value = -24; // Compress sounds above -24dB
		this.compressorNode.knee.value = 30;       // Smooth transition
		this.compressorNode.ratio.value = 8;       // Compression ratio
		this.compressorNode.attack.value = 0.005;  // React quickly to peaks
		this.compressorNode.release.value = 0.1;   // Release fairly quickly

		// gainNode is now created in constructor

		// Create 10-band equalizer
		this.equalizerBands = this.frequencies.map((freq) => {
			const filter = this.audioContext!.createBiquadFilter();
			filter.type = 'peaking';
			filter.frequency.value = freq;
			filter.Q.value = 1;
			filter.gain.value = 0; // 0 dB by default
			return filter;
		});
	}

	/**
	 * Load audio from a native file path (e.g. D:\Music\track.mp3).
	 * Uses Tauri's convertFileSrc to get a browser-accessible asset:// URL.
	 */
	public async loadAudioFromPath(filePath: string): Promise<void> {
		const url = convertFileSrc(filePath);
		await this.loadAudio(url);
	}

	public async loadAudio(url: string): Promise<void> {
		// Pause and tear down the old element, but keep the audio graph alive
		if (this.audioElement) {
			this.audioElement.pause();
			this.audioElement.src = '';
			this.audioElement = null;
		}

		// Disconnect old source if it exists
		if (this.sourceNode) {
			try { this.sourceNode.disconnect(); } catch { }
			this.sourceNode = null;
		}

		this.audioElement = new Audio(url);
		this.audioElement.crossOrigin = 'anonymous';
		this.audioElement.preload = 'auto';

		if (!this.audioContext) {
			this.initializeAudioContext();
		}

		if (this.audioContext!.state === 'suspended') {
			await this.audioContext!.resume();
		}

		this.sourceNode = this.audioContext!.createMediaElementSource(this.audioElement);

		// Disconnect gainNode from direct connection if it exists
		if (this.gainNode) {
			try {
				this.gainNode.disconnect();
			} catch (e) {
				// Ignore if already disconnected
			}
		}

		// Connect the audio graph: source -> EQ bands -> preGain -> analyser -> compressor -> gain -> destination
		let currentNode: AudioNode = this.sourceNode;

		for (const filter of this.equalizerBands) {
			currentNode.connect(filter);
			currentNode = filter;
		}

		currentNode.connect(this.preGainNode!);
		this.preGainNode!.connect(this.analyserNode!);
		this.analyserNode!.connect(this.compressorNode!);
		this.compressorNode!.connect(this.gainNode!);
		this.gainNode!.connect(this.audioContext!.destination);
	}

	public play(): void {
		if (this.audioContext?.state === 'suspended') {
			this.audioContext.resume();
		}
		this.audioElement?.play();
	}

	public pause(): void {
		this.audioElement?.pause();
	}

	public stop(): void {
		if (this.audioElement) {
			this.audioElement.pause();
			this.audioElement.currentTime = 0;
		}
	}

	public setVolume(volume: number): void {
		if (this.gainNode) {
			this.gainNode.gain.value = volume;
		}
		if (this.audioElement) {
			this.audioElement.volume = volume;
		}
	}

	public setEqualizerBand(bandIndex: number, gain: number): void {
		if (bandIndex >= 0 && bandIndex < this.equalizerBands.length) {
			this.equalizerBands[bandIndex].gain.value = gain;
		}
	}

	public getEqualizerBand(bandIndex: number): number {
		if (bandIndex >= 0 && bandIndex < this.equalizerBands.length) {
			return this.equalizerBands[bandIndex].gain.value;
		}
		return 0;
	}

	public resetEqualizer(): void {
		this.equalizerBands.forEach((band) => {
			band.gain.value = 0;
		});
	}

	/** Returns the AnalyserNode so the visualizer can read FFT data directly. */
	public getAnalyser(): AnalyserNode | null {
		return this.analyserNode;
	}

	public getFrequencyData(): Uint8Array {
		if (!this.analyserNode) {
			return new Uint8Array(0);
		}
		const bufferLength = this.analyserNode.frequencyBinCount;
		const dataArray = new Uint8Array(bufferLength);
		this.analyserNode.getByteFrequencyData(dataArray);
		return dataArray;
	}

	public getTimeDomainData(): Uint8Array {
		if (!this.analyserNode) {
			return new Uint8Array(0);
		}
		const bufferLength = this.analyserNode.frequencyBinCount;
		const dataArray = new Uint8Array(bufferLength);
		this.analyserNode.getByteTimeDomainData(dataArray);
		return dataArray;
	}

	public getFrequencies(): number[] {
		return this.frequencies;
	}

	public getCurrentTime(): number {
		return this.audioElement?.currentTime ?? 0;
	}

	public getDuration(): number {
		return this.audioElement?.duration ?? 0;
	}

	public seek(time: number): void {
		if (this.audioElement) {
			this.audioElement.currentTime = time;
		}
	}

	public isPlaying(): boolean {
		return this.audioElement ? !this.audioElement.paused : false;
	}

	public onEnded(callback: () => void): void {
		if (this.audioElement) {
			this.audioElement.onended = callback;
		}
	}

	public onTimeUpdate(callback: () => void): void {
		if (this.audioElement) {
			this.audioElement.ontimeupdate = callback;
		}
	}

	public dispose(): void {
		if (this.audioElement) {
			this.audioElement.pause();
			this.audioElement = null;
		}

		if (this.audioContext) {
			this.audioContext.close();
			this.audioContext = null;
		}

		this.sourceNode = null;
		this.preGainNode = null;
		this.analyserNode = null;
		this.compressorNode = null;
		this.gainNode = null;
		this.equalizerBands = [];
	}
}
