use rodio::{Decoder, OutputStream, Sink};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use symphonia::core::codecs::CODEC_TYPE_NULL;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use std::sync::{Arc, Mutex};

// Wrapper to make OutputStream Send on Windows
struct SendOutputStream(OutputStream);

unsafe impl Send for SendOutputStream {}

pub struct AudioPlayer {
    stream: Option<SendOutputStream>,
    sink: Option<Sink>,
    frequency_data: Arc<Mutex<Vec<u8>>>,
}

impl AudioPlayer {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(AudioPlayer {
            stream: None,
            sink: None,
            frequency_data: Arc::new(Mutex::new(vec![0u8; 64])),
        })
    }

    pub fn play_file(&mut self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        // Stop any currently playing audio
        if let Some(sink) = &self.sink {
            sink.stop();
        }

        // Drop old stream and sink
        self.sink = None;
        self.stream = None;

        // Create new stream and sink for each play
        let (stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| format!("Failed to initialize audio device: {}", e))?;
        let sink = Sink::try_new(&stream_handle)
            .map_err(|e| format!("Failed to create audio sink: {}", e))?;

        // Load the audio file
        let file = File::open(path)?;
        let source = Decoder::new(BufReader::new(file))
            .map_err(|e| format!("Failed to decode audio file: {}", e))?;

        // Append the source to the sink
        sink.append(source);
        sink.play();

        // Keep both alive
        self.stream = Some(SendOutputStream(stream));
        self.sink = Some(sink);
        Ok(())
    }

    pub fn pause(&self) {
        if let Some(sink) = &self.sink {
            sink.pause();
        }
    }

    pub fn resume(&self) {
        if let Some(sink) = &self.sink {
            sink.play();
        }
    }

    pub fn stop(&mut self) {
        if let Some(sink) = &self.sink {
            sink.stop();
        }
    }

    pub fn set_volume(&self, volume: f32) {
        if let Some(sink) = &self.sink {
            sink.set_volume(volume);
        }
    }

    pub fn is_playing(&self) -> bool {
        if let Some(sink) = &self.sink {
            !sink.is_paused() && !sink.empty()
        } else {
            false
        }
    }

    pub fn seek(&self, position: f64) {
        if let Some(sink) = &self.sink {
            sink.try_seek(std::time::Duration::from_secs_f64(position)).ok();
        }
    }

    pub fn get_frequency_data(&self) -> Vec<u8> {
        self.frequency_data.lock().unwrap().clone()
    }

    pub fn update_frequency_data(&self, is_playing: bool, _volume: f32) {
        let mut data = self.frequency_data.lock().unwrap();
        if !is_playing {
            data.fill(0);
            return;
        }

        // Use simulated data since real FFT requires sample capture which breaks seeking
        Self::generate_simulated_data(&mut data);
    }

    fn generate_simulated_data(data: &mut [u8]) {
        use std::time::{SystemTime, UNIX_EPOCH};
        let time_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        for i in 0..data.len() {
            let seed = time_ns.wrapping_mul((i as u64).wrapping_add(1));
            let r1 = (seed % 1000) as f32 / 1000.0;
            let r2 = ((seed / 1000) % 1000) as f32 / 1000.0;

            let freq_envelope = 1.0 - (i as f32 / data.len() as f32).powf(0.5);
            let base = 40.0;
            let dynamic = r1 * 80.0 + r2 * 60.0;

            let smooth = if i > 0 { data[i - 1] as f32 * 0.3 } else { 0.0 };

            let value = base + dynamic * freq_envelope + smooth;
            data[i] = value.min(255.0).max(0.0) as u8;
        }
    }

    pub fn get_duration(path: &Path) -> Result<f64, Box<dyn std::error::Error>> {
        let file = File::open(path)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        
        let hint = Hint::new();
        let meta_opts: MetadataOptions = Default::default();
        let fmt_opts: FormatOptions = Default::default();

        let probed = symphonia::default::get_probe().format(
            &hint,
            mss,
            &fmt_opts,
            &meta_opts,
        )?;

        let format = probed.format;
        let track = format.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL).ok_or("No audio track found")?;
        let codec_params = &track.codec_params;

        if let (Some(time_base), Some(n_frames)) = (codec_params.time_base, codec_params.n_frames) {
            Ok(n_frames as f64 * time_base.numer as f64 / time_base.denom as f64)
        } else {
            Ok(0.0)
        }
    }
}
