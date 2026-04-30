<script lang="ts">
	import { onMount, onDestroy } from 'svelte';

	let { analyser = null, type = 'bars', height = 200 }: {
		analyser?: AnalyserNode | null;
		type?: 'bars' | 'wave' | 'circle';
		height?: number;
	} = $props();

	let canvas: HTMLCanvasElement;
	let ctx: CanvasRenderingContext2D | null = null;
	let animationId: number;
	
	// Create arrays for data to avoid allocating them every frame
	let frequencyData = new Uint8Array(0);
	
	// Added smoothing array to make animations look premium
	let smoothedData: number[] = [];

	$effect(() => {
		if (analyser) {
			const bufferLength = analyser.frequencyBinCount;
			frequencyData = new Uint8Array(bufferLength);
			smoothedData = new Array(bufferLength).fill(0);
		}
	});

	onMount(() => {
		ctx = canvas.getContext('2d');
		resizeCanvas();
		startVisualization();
		window.addEventListener('resize', resizeCanvas);
	});

	onDestroy(() => {
		cancelAnimationFrame(animationId);
		window.removeEventListener('resize', resizeCanvas);
	});

	function resizeCanvas() {
		if (!canvas || !canvas.parentElement) return;
		const parentWidth = canvas.parentElement.clientWidth || 200;
		canvas.width = parentWidth;
		canvas.height = height;
	}

	function startVisualization() {
		function draw() {
			animationId = requestAnimationFrame(draw);
			
			if (!ctx) return;
			
			if (analyser && frequencyData.length > 0) {
				analyser.getByteFrequencyData(frequencyData);
				
				// Smooth the data (Exponential Moving Average)
				const smoothingFactor = 0.6;
				for (let i = 0; i < frequencyData.length; i++) {
					smoothedData[i] = smoothedData[i] * smoothingFactor + frequencyData[i] * (1 - smoothingFactor);
				}
			} else {
				// Fade to 0 if no audio
				for (let i = 0; i < smoothedData.length; i++) {
					smoothedData[i] *= 0.9;
				}
			}

			if (type === 'bars') {
				drawBars();
			} else if (type === 'wave') {
				drawWave();
			} else {
				drawCircle();
			}
		}

		draw();
	}

	function drawBars() {
		if (!ctx) return;

		const width = canvas.width;
		const height = canvas.height;

		ctx.clearRect(0, 0, width, height);

		if (smoothedData.length === 0) return;

		// Focus on lower/mid frequencies, top ones are often empty for music
		const usefulDataLength = Math.min(smoothedData.length, 256);
		const barCount = 64; 
		const barWidth = width / barCount;
		const step = Math.floor(usefulDataLength / barCount);

		for (let i = 0; i < barCount; i++) {
			const dataIndex = i * step;
			let value = smoothedData[dataIndex] || 0;
			
			// Boost lower frequencies slightly to make them pop more
			const boost = i < 10 ? 1.2 : 1.0;
			const barHeight = Math.min(height, (value / 255) * height * boost);

			const x = i * barWidth;
			const y = height - barHeight;

			// Premium gradient
			const gradient = ctx.createLinearGradient(x, height, x, y);
			gradient.addColorStop(0, 'rgba(255, 255, 255, 0.1)'); // Tail
			gradient.addColorStop(0.5, '#aaaaaa');
			gradient.addColorStop(1, '#ffffff');

			ctx.fillStyle = gradient;
			
			// Add rounded corners to bars
			ctx.beginPath();
			const roundedRadius = Math.min(barWidth / 2 - 1, 4);
			ctx.roundRect(x, y, barWidth - 2, barHeight, [roundedRadius, roundedRadius, 0, 0]);
			ctx.fill();
			
			// Glowing top cap
			if (barHeight > 2) {
				ctx.fillStyle = '#fff';
				ctx.shadowColor = '#ffffff';
				ctx.shadowBlur = 10;
				ctx.fillRect(x, y, barWidth - 2, 2);
				ctx.shadowBlur = 0; // reset
			}
		}
	}

	function drawWave() {
		if (!ctx) return;

		const width = canvas.width;
		const height = canvas.height;

		// Add slightly transparent clear to get a motion blur effect
		ctx.fillStyle = 'rgba(24, 24, 37, 0.3)';
		ctx.fillRect(0, 0, width, height);

		if (smoothedData.length === 0) return;
		
		// If using analyser for wave, we should ideally use getTimeDomainData,
		// but since we only setup getByteFrequencyData right now, let's make a mirrored frequency wave
		
		const usefulDataLength = 128;
		const sliceWidth = width / (usefulDataLength * 2);
		let x = 0;

		ctx.lineWidth = 3;
		
		const gradient = ctx.createLinearGradient(0, 0, width, 0);
		gradient.addColorStop(0, '#cccccc');
		gradient.addColorStop(0.5, '#ffffff');
		gradient.addColorStop(1, '#cccccc');
		ctx.strokeStyle = gradient;

		ctx.beginPath();
		
		const centerY = height / 2;

		// Left side (mirrored)
		for (let i = usefulDataLength - 1; i >= 0; i--) {
			const v = (smoothedData[i] || 0) / 255.0;
			const displacement = v * (height / 2);
			const y = centerY - displacement;

			if (i === usefulDataLength - 1) {
				ctx.moveTo(x, y);
			} else {
				ctx.lineTo(x, y);
			}
			x += sliceWidth;
		}

		// Right side
		for (let i = 0; i < usefulDataLength; i++) {
			const v = (smoothedData[i] || 0) / 255.0;
			const displacement = v * (height / 2);
			const y = centerY - displacement;

			ctx.lineTo(x, y);
			x += sliceWidth;
		}

		ctx.stroke();
		
		// Add a subtle glow
		ctx.shadowColor = '#ffffff';
		ctx.shadowBlur = 15;
		ctx.stroke();
		ctx.shadowBlur = 0;
	}
	
	function drawCircle() {
		if (!ctx) return;

		const width = canvas.width;
		const height = canvas.height;
		const centerX = width / 2;
		const centerY = height / 2;

		ctx.clearRect(0, 0, width, height);

		if (smoothedData.length === 0) return;
		
		const usefulDataLength = 120;
		const barCount = 60;
		const step = Math.floor(usefulDataLength / barCount);
		
		const baseRadius = Math.min(width, height) / 4;
		
		// Map an average bass volume to a scale factor to make the whole circle pulse
		let bassSum = 0;
		for(let i=0; i<10; i++) {
			bassSum += smoothedData[i] || 0;
		}
		const pulseFactor = 1 + (bassSum / (255 * 10)) * 0.2;
		const currentRadius = baseRadius * pulseFactor;

		for (let i = 0; i < barCount; i++) {
			const dataIndex = i * step;
			const value = smoothedData[dataIndex] || 0;
			
			const barHeight = (value / 255) * (height / 2.5);
			
			// We draw full circle (360 degrees) -> Math.PI * 2
			const angle = (i * (Math.PI * 2)) / barCount;
			
			// To rotate it so 0 frequency is on top, subtract PI/2
			const rotatedAngle = angle - (Math.PI / 2);
			
			const x1 = centerX + Math.cos(rotatedAngle) * currentRadius;
			const y1 = centerY + Math.sin(rotatedAngle) * currentRadius;
			
			const x2 = centerX + Math.cos(rotatedAngle) * (currentRadius + barHeight);
			const y2 = centerY + Math.sin(rotatedAngle) * (currentRadius + barHeight);

			ctx.beginPath();
			ctx.moveTo(x1, y1);
			ctx.lineTo(x2, y2);
			
			ctx.strokeStyle = `rgba(255, 255, 255, ${0.4 + (value / 255) * 0.6})`;
			ctx.lineWidth = 4;
			ctx.lineCap = 'round';
			ctx.stroke();
			
			// Inner ring glow based on low freqs
			if(i === 0) {
				ctx.beginPath();
				ctx.arc(centerX, centerY, currentRadius - 5, 0, Math.PI * 2);
				ctx.strokeStyle = `rgba(255, 255, 255, ${Math.min(0.5, pulseFactor - 1)})`;
				ctx.lineWidth = 2;
				ctx.shadowBlur = 20;
				ctx.shadowColor = '#ffffff';
				ctx.stroke();
				ctx.shadowBlur = 0;
			}
		}
	}
</script>

<canvas bind:this={canvas} class="w-full"></canvas>

<style>
	canvas {
		display: block;
		width: 100%;
		height: 100%;
		/* Slight rounded corners for a premium feel */
		border-radius: 8px;
	}
</style>
