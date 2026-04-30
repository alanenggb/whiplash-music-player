<script lang="ts">
	import type { AudioProcessor } from '../audioProcessor';

	export let audioProcessor: AudioProcessor;

	const frequencies = audioProcessor.getFrequencies();
	let gains = frequencies.map(() => 0);

	function updateGain(index: number, event: Event) {
		const target = event.target as HTMLInputElement;
		gains[index] = parseFloat(target.value);
		audioProcessor.setEqualizerBand(index, gains[index]);
	}

	function resetEqualizer() {
		gains = frequencies.map(() => 0);
		frequencies.forEach((_, i) => audioProcessor.setEqualizerBand(i, 0));
	}
</script>

<div class="equalizer-container">
	<div class="equalizer-header">
		<h3>Equalizer</h3>
		<button on:click={resetEqualizer} class="reset-btn">Reset</button>
	</div>
	<div class="bands">
		{#each frequencies as freq, i}
			<div class="band">
				<div class="band-label">{freq >= 1000 ? `${freq / 1000}k` : freq}Hz</div>
				<input
					type="range"
					min="-12"
					max="12"
					step="0.5"
					value={gains[i]}
					on:input={(e) => updateGain(i, e)}
					class="band-slider"
				/>
				<div class="band-value">{gains[i].toFixed(1)} dB</div>
			</div>
		{/each}
	</div>
</div>

<style>
	.equalizer-container {
		background: #1a1a1a;
		border-radius: 8px;
		padding: 16px;
		margin: 16px 0;
	}

	.equalizer-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 16px;
	}

	.equalizer-header h3 {
		margin: 0;
		color: #e0e0e0;
	}

	.reset-btn {
		background: #3e3e3e;
		color: #e0e0e0;
		border: none;
		padding: 6px 12px;
		border-radius: 4px;
		cursor: pointer;
	}

	.reset-btn:hover {
		background: #585b70;
	}

	.bands {
		display: flex;
		gap: 8px;
		overflow-x: auto;
		padding-bottom: 8px;
	}

	.band {
		display: flex;
		flex-direction: column;
		align-items: center;
		min-width: 60px;
	}

	.band-label {
		font-size: 12px;
		color: #a0a0a0;
		margin-bottom: 8px;
	}

	.band-slider {
		width: 100%;
		height: 120px;
		-webkit-appearance: none;
		appearance: none;
		background: #2d2d2d;
		border-radius: 4px;
		outline: none;
	}

	.band-slider::-webkit-slider-thumb {
		-webkit-appearance: none;
		appearance: none;
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: #ffffff;
		cursor: pointer;
	}

	.band-slider::-moz-range-thumb {
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: #ffffff;
		cursor: pointer;
		border: none;
	}

	.band-value {
		font-size: 11px;
		color: #a0a0a0;
		margin-top: 8px;
	}
</style>
