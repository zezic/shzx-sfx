//! Native output and the lifetime of its dedicated owner thread.

use std::{
    num::NonZeroU32,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use cpal::{
    FromSample, Sample, SampleFormat, SizedSample,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use crossbeam_queue::ArrayQueue;

use crate::{Command, Error, RenderConfig, Renderer, Result, Snapshot};

const COMMANDS_PER_CALLBACK: usize = 64;
const RENDER_FRAMES: usize = 256;
const CONTROL_POLL: Duration = Duration::from_millis(20);
const SHUTDOWN_FADE: Duration = Duration::from_millis(30);
const SHUTDOWN_DEADLINE: Duration = Duration::from_millis(250);

/// Requested device callback size. The host can still deliver varying extents.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BufferSize {
    #[default]
    SystemDefault,
    Fixed(NonZeroU32),
}

/// Native playback settings. The device's negotiated rate overrides
/// `renderer.sample_rate`; all DSP is prepared before playback starts.
#[derive(Clone, Debug)]
pub struct AudioConfig {
    /// Exact output device name, or the system default when omitted.
    pub device: Option<String>,
    pub renderer: RenderConfig,
    pub buffer_size: BufferSize,
    /// Maximum pending commands, from 1 through 65,536.
    pub queue_capacity: usize,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            device: None,
            renderer: RenderConfig::default(),
            buffer_size: BufferSize::SystemDefault,
            queue_capacity: 256,
        }
    }
}

/// An available native output device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputDevice {
    pub name: String,
    pub is_default: bool,
}

/// The actual output configuration chosen by the device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputInfo {
    pub device: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: String,
    /// Requested buffer setting; actual callback lengths remain host-controlled.
    pub buffer_size: BufferSize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioStatus {
    Running,
    Stopped,
    Failed(cpal::ErrorKind),
}

/// Observed callback extents, independent of the requested buffer size.
/// Fields are read concurrently and are intended for live diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CallbackStats {
    pub last_frames: usize,
    pub min_frames: usize,
    pub max_frames: usize,
    pub callbacks: u64,
}

struct Shared {
    commands: ArrayQueue<Command>,
    snapshots: ArrayQueue<Snapshot>,
    failures: ArrayQueue<cpal::ErrorKind>,
    notices: ArrayQueue<cpal::ErrorKind>,
    accepting: AtomicBool,
    fade_requested: AtomicBool,
    fade_completed: AtomicBool,
    callback_last: AtomicUsize,
    callback_min: AtomicUsize,
    callback_max: AtomicUsize,
    callback_count: AtomicU64,
    // Only the caller and the owner thread touch this mutex. Neither CPAL
    // callback takes a lock or formats an error message.
    status: Mutex<AudioStatus>,
}

impl Shared {
    fn new(capacity: usize) -> Self {
        Self {
            commands: ArrayQueue::new(capacity),
            snapshots: ArrayQueue::new(2),
            failures: ArrayQueue::new(8),
            notices: ArrayQueue::new(32),
            accepting: AtomicBool::new(false),
            fade_requested: AtomicBool::new(false),
            fade_completed: AtomicBool::new(false),
            callback_last: AtomicUsize::new(0),
            callback_min: AtomicUsize::new(0),
            callback_max: AtomicUsize::new(0),
            callback_count: AtomicU64::new(0),
            status: Mutex::new(AudioStatus::Stopped),
        }
    }

    fn set_status(&self, value: AudioStatus) {
        *self
            .status
            .lock()
            .expect("audio status owner does not panic") = value;
    }

    fn report(&self, kind: cpal::ErrorKind) {
        let _ = self.notices.force_push(kind);
        if !matches!(
            kind,
            cpal::ErrorKind::Xrun
                | cpal::ErrorKind::RealtimeDenied
                | cpal::ErrorKind::DeviceChanged
        ) {
            let _ = self.failures.push(kind);
            self.accepting.store(false, Ordering::Release);
        }
    }
}

/// Cloneable, bounded, nonblocking control access to the audio callback.
#[derive(Clone)]
pub struct AudioHandle {
    shared: Arc<Shared>,
}

impl AudioHandle {
    /// Enqueue a command. Success confirms queueing, not audible completion.
    /// A full queue is reported explicitly so the UI can coalesce updates.
    pub fn send(&self, command: Command) -> Result<()> {
        if !self.shared.accepting.load(Ordering::Acquire) {
            return Err(Error::Stopped);
        }
        self.shared
            .commands
            .push(command)
            .map_err(|_| Error::QueueFull)
    }
}

/// Owns the native output stream and its dedicated lifecycle thread.
/// Dropping the owner stops and joins the thread, even if handles remain alive.
#[must_use = "keep the audio engine alive for as long as playback is required"]
pub struct AudioEngine {
    shared: Arc<Shared>,
    stop: Sender<()>,
    thread: Option<JoinHandle<Result<()>>>,
    output: OutputInfo,
}

impl AudioEngine {
    /// Start playback, returning only after device setup and `play()` succeed.
    pub fn start(config: AudioConfig) -> Result<Self> {
        if !(1..=65_536).contains(&config.queue_capacity) {
            return Err(Error::InvalidParameter {
                name: "queue_capacity",
                value: config.queue_capacity as f64,
                expected: "an integer from 1 through 65536",
            });
        }
        let shared = Arc::new(Shared::new(config.queue_capacity));
        let (stop, shutdown) = mpsc::channel();
        let (ready, startup) = mpsc::sync_channel(1);
        let owner_shared = Arc::clone(&shared);
        let owner = thread::Builder::new()
            .name("shzx-sfx audio".into())
            .spawn(move || run(config, owner_shared, shutdown, ready))?;

        match startup.recv() {
            Ok(Ok(output)) => Ok(Self {
                shared,
                stop,
                thread: Some(owner),
                output,
            }),
            Ok(Err(error)) => {
                owner.join().map_err(|_| Error::ThreadPanicked)??;
                Err(error)
            }
            Err(_) => {
                owner.join().map_err(|_| Error::ThreadPanicked)??;
                Err(Error::Audio("audio owner exited during startup".into()))
            }
        }
    }

    #[must_use]
    pub fn handle(&self) -> AudioHandle {
        AudioHandle {
            shared: Arc::clone(&self.shared),
        }
    }

    #[must_use]
    pub fn output_info(&self) -> &OutputInfo {
        &self.output
    }

    #[must_use]
    pub fn status(&self) -> AudioStatus {
        *self
            .shared
            .status
            .lock()
            .expect("audio status owner does not panic")
    }

    /// Read observed callback frame counts without timing the callback.
    /// These metrics do not measure CPU use or identify a processor core type.
    #[must_use]
    pub fn callback_stats(&self) -> CallbackStats {
        let callbacks = self.shared.callback_count.load(Ordering::Acquire);
        CallbackStats {
            last_frames: self.shared.callback_last.load(Ordering::Relaxed),
            min_frames: self.shared.callback_min.load(Ordering::Relaxed),
            max_frames: self.shared.callback_max.load(Ordering::Relaxed),
            callbacks,
        }
    }

    /// Take the newest completed callback snapshot, discarding older ones.
    pub fn latest_snapshot(&mut self) -> Option<Snapshot> {
        let mut latest = None;
        // The producer may continue publishing while this is called. A fixed
        // bound keeps polling nonblocking even with unusually small buffers.
        for _ in 0..self.shared.snapshots.capacity() {
            match self.shared.snapshots.pop() {
                Some(snapshot) => latest = Some(snapshot),
                None => break,
            }
        }
        latest
    }

    /// Retrieve a device notice, including underruns and terminal failures.
    /// Recoverable notices do not interrupt playback.
    pub fn take_notice(&self) -> Option<cpal::ErrorKind> {
        self.shared.notices.pop()
    }

    /// Stop accepting commands, release the stream, and join its owner.
    /// Shutdown uses a separate channel and cannot be blocked by a full queue.
    pub fn shutdown(mut self) -> Result<()> {
        self.stop_and_join()
    }

    fn stop_and_join(&mut self) -> Result<()> {
        self.shared.accepting.store(false, Ordering::Release);
        if let Some(owner) = self.thread.take() {
            let _ = self.stop.send(());
            owner.join().map_err(|_| Error::ThreadPanicked)??;
        }
        Ok(())
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        let _ = self.stop_and_join();
    }
}

struct OwnerLifetime(Arc<Shared>);

impl Drop for OwnerLifetime {
    fn drop(&mut self) {
        self.0.accepting.store(false, Ordering::Release);
        let mut status = self
            .0
            .status
            .lock()
            .expect("audio status owner does not panic");
        if *status == AudioStatus::Running {
            *status = AudioStatus::Stopped;
        }
    }
}

fn run(
    config: AudioConfig,
    shared: Arc<Shared>,
    shutdown: Receiver<()>,
    ready: mpsc::SyncSender<Result<OutputInfo>>,
) -> Result<()> {
    let _lifetime = OwnerLifetime(Arc::clone(&shared));
    let (stream, output) = match prepare(config, Arc::clone(&shared)) {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = ready.send(Err(error));
            return Ok(());
        }
    };
    shared.accepting.store(true, Ordering::Release);
    shared.set_status(AudioStatus::Running);
    if let Err(error) = stream.play() {
        shared.set_status(AudioStatus::Failed(error.kind()));
        let _ = ready.send(Err(Error::Audio(error.to_string())));
        return Ok(());
    }
    if let Some(kind) = shared.failures.pop() {
        shared.set_status(AudioStatus::Failed(kind));
        let _ = ready.send(Err(Error::Audio(kind.to_string())));
        return Ok(());
    }
    if ready.send(Ok(output)).is_err() {
        return Ok(());
    }
    loop {
        let stopping = match shutdown.recv_timeout(CONTROL_POLL) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => true,
            Err(RecvTimeoutError::Timeout) => false,
        };
        if let Some(kind) = shared.failures.pop() {
            shared.set_status(AudioStatus::Failed(kind));
            return Err(Error::Audio(kind.to_string()));
        }
        if stopping {
            shared.fade_requested.store(true, Ordering::Release);
            let deadline = Instant::now() + SHUTDOWN_DEADLINE;
            while !shared.fade_completed.load(Ordering::Acquire)
                && shared.failures.is_empty()
                && Instant::now() < deadline
            {
                thread::sleep(Duration::from_millis(2));
            }
            if let Some(kind) = shared.failures.pop() {
                shared.set_status(AudioStatus::Failed(kind));
                return Err(Error::Audio(kind.to_string()));
            }
            return Ok(());
        }
    }
}

fn prepare(config: AudioConfig, shared: Arc<Shared>) -> Result<(cpal::Stream, OutputInfo)> {
    let host = cpal::default_host();
    let device = select_device(&host, config.device.as_deref())?;
    let supported = device.default_output_config().map_err(audio_error)?;
    let mut stream_config = supported.config();
    stream_config.buffer_size = requested_buffer(config.buffer_size, supported.buffer_size())?;
    if stream_config.channels == 0 {
        return Err(Error::Audio("output device reports zero channels".into()));
    }
    let mut render_config = config.renderer;
    render_config.sample_rate = stream_config.sample_rate;
    let renderer = Renderer::new(render_config)?;
    let format = supported.sample_format();
    let output = OutputInfo {
        device: device.description().map_err(audio_error)?.name().to_owned(),
        sample_rate: stream_config.sample_rate,
        channels: stream_config.channels,
        sample_format: format.to_string(),
        buffer_size: config.buffer_size,
    };
    let stream = match format {
        SampleFormat::I8 => build_stream::<i8>(&device, stream_config, renderer, shared),
        SampleFormat::I16 => build_stream::<i16>(&device, stream_config, renderer, shared),
        SampleFormat::I24 => build_stream::<cpal::I24>(&device, stream_config, renderer, shared),
        SampleFormat::I32 => build_stream::<i32>(&device, stream_config, renderer, shared),
        SampleFormat::I64 => build_stream::<i64>(&device, stream_config, renderer, shared),
        SampleFormat::U8 => build_stream::<u8>(&device, stream_config, renderer, shared),
        SampleFormat::U16 => build_stream::<u16>(&device, stream_config, renderer, shared),
        SampleFormat::U24 => build_stream::<cpal::U24>(&device, stream_config, renderer, shared),
        SampleFormat::U32 => build_stream::<u32>(&device, stream_config, renderer, shared),
        SampleFormat::U64 => build_stream::<u64>(&device, stream_config, renderer, shared),
        SampleFormat::F32 => build_stream::<f32>(&device, stream_config, renderer, shared),
        SampleFormat::F64 => build_stream::<f64>(&device, stream_config, renderer, shared),
        _ => {
            return Err(Error::Audio(format!(
                "unsupported output sample format: {format}"
            )));
        }
    }?;
    Ok((stream, output))
}

fn build_stream<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut renderer: Renderer,
    shared: Arc<Shared>,
) -> Result<cpal::Stream> {
    let errors = Arc::clone(&shared);
    let channels = usize::from(config.channels);
    let mut tile = [[0.0_f32; 2]; RENDER_FRAMES];
    let mut shutdown_gain = 1.0_f32;
    let fade_step = 1.0 / (config.sample_rate as f32 * SHUTDOWN_FADE.as_secs_f32());
    let mut callback_stats = CallbackStats::default();
    device
        .build_output_stream(
            config,
            move |output: &mut [T], _| {
                for _ in 0..COMMANDS_PER_CALLBACK {
                    match shared.commands.pop() {
                        Some(command) => renderer.apply(command),
                        None => break,
                    }
                }
                assert_eq!(
                    output.len() % channels,
                    0,
                    "device output contains complete frames"
                );
                let frames = output.len() / channels;
                callback_stats.min_frames = if callback_stats.callbacks == 0 {
                    frames
                } else {
                    callback_stats.min_frames.min(frames)
                };
                callback_stats.max_frames = callback_stats.max_frames.max(frames);
                callback_stats.callbacks += 1;
                shared
                    .callback_min
                    .store(callback_stats.min_frames, Ordering::Relaxed);
                shared
                    .callback_max
                    .store(callback_stats.max_frames, Ordering::Relaxed);
                shared.callback_last.store(frames, Ordering::Relaxed);
                shared
                    .callback_count
                    .store(callback_stats.callbacks, Ordering::Release);
                for destination in output.chunks_mut(RENDER_FRAMES * channels) {
                    let count = destination.len() / channels;
                    let stereo = &mut tile[..count];
                    if shutdown_gain > 0.0 {
                        renderer.render(stereo);
                        if shared.fade_requested.load(Ordering::Acquire) {
                            apply_fade(stereo, &mut shutdown_gain, fade_step);
                        }
                    } else {
                        stereo.fill([0.0; 2]);
                    }
                    write_output(destination, channels, stereo);
                }
                if shutdown_gain == 0.0 {
                    shared.fade_completed.store(true, Ordering::Release);
                }
                let _ = shared.snapshots.force_push(renderer.snapshot());
            },
            move |error| errors.report(error.kind()),
            Some(Duration::from_secs(5)),
        )
        .map_err(audio_error)
}

fn write_output<T: Sample + FromSample<f32>>(
    output: &mut [T],
    channels: usize,
    stereo: &[[f32; 2]],
) {
    assert_eq!(
        output.len(),
        stereo.len() * channels,
        "device output span matches the rendered stereo frames"
    );
    match channels {
        1 => {
            for (sample, &[left, right]) in output.iter_mut().zip(stereo) {
                *sample = T::from_sample((left + right) * 0.5);
            }
        }
        2 => {
            for (sample, &value) in output.iter_mut().zip(stereo.as_flattened()) {
                *sample = T::from_sample(value);
            }
        }
        _ => {
            for (frame, &[left, right]) in output.chunks_exact_mut(channels).zip(stereo) {
                frame[0] = T::from_sample(left);
                frame[1] = T::from_sample(right);
                frame[2..].fill(T::EQUILIBRIUM);
            }
        }
    }
}

fn apply_fade(stereo: &mut [[f32; 2]], gain: &mut f32, step: f32) {
    for frame in stereo {
        *gain = (*gain - step).max(0.0);
        frame[0] *= *gain;
        frame[1] *= *gain;
    }
}

fn requested_buffer(
    requested: BufferSize,
    supported: &cpal::SupportedBufferSize,
) -> Result<cpal::BufferSize> {
    match requested {
        BufferSize::SystemDefault => Ok(cpal::BufferSize::Default),
        BufferSize::Fixed(frames) => {
            if let cpal::SupportedBufferSize::Range { min, max } = supported
                && !(*min..=*max).contains(&frames.get())
            {
                return Err(Error::Audio(format!(
                    "requested buffer has {} frames; output device supports {min}..={max}",
                    frames.get()
                )));
            }
            Ok(cpal::BufferSize::Fixed(frames.get()))
        }
    }
}

fn select_device(host: &cpal::Host, name: Option<&str>) -> Result<cpal::Device> {
    let Some(name) = name else {
        return host
            .default_output_device()
            .ok_or_else(|| Error::Audio("no default audio output device is available".into()));
    };
    let mut found = None;
    for device in host.output_devices().map_err(audio_error)? {
        if device.description().map_err(audio_error)?.name() == name {
            if found.is_some() {
                return Err(Error::Audio(format!(
                    "output device name is ambiguous: {name}"
                )));
            }
            found = Some(device);
        }
    }
    found.ok_or_else(|| Error::Audio(format!("audio output device was not found: {name}")))
}

/// Enumerate the current native output devices without opening a stream.
pub fn output_devices() -> Result<Vec<OutputDevice>> {
    let host = cpal::default_host();
    let default_id = host
        .default_output_device()
        .map(|device| device.id())
        .transpose()
        .map_err(audio_error)?;
    host.output_devices()
        .map_err(audio_error)?
        .map(|device| {
            Ok(OutputDevice {
                name: device.description().map_err(audio_error)?.name().to_owned(),
                is_default: default_id.as_ref() == Some(&device.id().map_err(audio_error)?),
            })
        })
        .collect()
}

fn audio_error(error: cpal::Error) -> Error {
    Error::Audio(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_layout_folds_mono_and_silences_unused_unsigned_channels() {
        let mut mono = [0.0_f32; 3];
        write_output(&mut mono, 1, &[[0.8, -0.4]; 3]);
        assert_eq!(mono, [0.2; 3]);

        let mut surround = [0_u16; 12];
        write_output(&mut surround, 6, &[[0.5, -0.5]; 2]);
        for frame in surround.as_chunks::<6>().0 {
            assert!(frame[0] > u16::EQUILIBRIUM);
            assert!(frame[1] < u16::EQUILIBRIUM);
            assert_eq!(&frame[2..], &[u16::EQUILIBRIUM; 4]);
        }
    }

    #[test]
    fn notices_distinguish_recoverable_and_terminal_errors() {
        let shared = Shared::new(1);
        shared.accepting.store(true, Ordering::Release);
        shared.report(cpal::ErrorKind::Xrun);
        assert!(shared.accepting.load(Ordering::Acquire));
        assert!(shared.failures.pop().is_none());
        assert_eq!(shared.notices.pop(), Some(cpal::ErrorKind::Xrun));

        shared.report(cpal::ErrorKind::DeviceNotAvailable);
        assert!(!shared.accepting.load(Ordering::Acquire));
        assert_eq!(
            shared.failures.pop(),
            Some(cpal::ErrorKind::DeviceNotAvailable)
        );
    }

    #[test]
    fn owner_lifetime_stops_handles_and_preserves_failure() {
        let shared = Arc::new(Shared::new(1));
        shared.accepting.store(true, Ordering::Release);
        shared.set_status(AudioStatus::Running);
        drop(OwnerLifetime(Arc::clone(&shared)));
        assert!(!shared.accepting.load(Ordering::Acquire));
        assert_eq!(*shared.status.lock().unwrap(), AudioStatus::Stopped);

        shared.set_status(AudioStatus::Failed(cpal::ErrorKind::BackendError));
        drop(OwnerLifetime(Arc::clone(&shared)));
        assert_eq!(
            *shared.status.lock().unwrap(),
            AudioStatus::Failed(cpal::ErrorKind::BackendError)
        );
    }

    #[test]
    fn invalid_queue_capacity_is_rejected_before_device_access() {
        let result = AudioEngine::start(AudioConfig {
            queue_capacity: 0,
            ..AudioConfig::default()
        });
        assert!(matches!(
            result,
            Err(Error::InvalidParameter {
                name: "queue_capacity",
                ..
            })
        ));
    }

    #[test]
    fn a_full_command_queue_cannot_block_shutdown_or_leave_handles_live() {
        let shared = Arc::new(Shared::new(1));
        shared.accepting.store(true, Ordering::Release);
        shared.set_status(AudioStatus::Running);
        let completed = Arc::new(AtomicBool::new(false));
        let owner_completed = Arc::clone(&completed);
        let owner_shared = Arc::clone(&shared);
        let (stop, shutdown) = mpsc::channel();
        let owner = thread::spawn(move || {
            let _lifetime = OwnerLifetime(owner_shared);
            shutdown.recv().unwrap();
            owner_completed.store(true, Ordering::Release);
            Ok(())
        });
        let engine = AudioEngine {
            shared,
            stop,
            thread: Some(owner),
            output: OutputInfo {
                device: "test output".into(),
                sample_rate: 48_000,
                channels: 2,
                sample_format: "f32".into(),
                buffer_size: BufferSize::SystemDefault,
            },
        };
        let handle = engine.handle();
        let command = Command::Mode(crate::Mode::Dialog);
        handle.send(command).unwrap();
        assert!(matches!(handle.send(command), Err(Error::QueueFull)));
        engine.shutdown().unwrap();
        assert!(completed.load(Ordering::Acquire));
        assert!(matches!(handle.send(command), Err(Error::Stopped)));
    }

    #[test]
    fn shutdown_fade_reaches_silence_monotonically() {
        let mut stereo = [[1.0, -1.0]; 32];
        let mut gain = 1.0;
        apply_fade(&mut stereo, &mut gain, 1.0 / 16.0);
        assert_eq!(gain, 0.0);
        assert!(stereo.windows(2).all(|pair| pair[1][0] <= pair[0][0]));
        assert_eq!(&stereo[16..], &[[0.0; 2]; 16]);
    }

    #[test]
    fn fixed_buffers_respect_known_device_limits() {
        let supported = cpal::SupportedBufferSize::Range { min: 64, max: 1024 };
        assert!(matches!(
            requested_buffer(BufferSize::SystemDefault, &supported).unwrap(),
            cpal::BufferSize::Default
        ));
        assert!(matches!(
            requested_buffer(BufferSize::Fixed(NonZeroU32::new(256).unwrap()), &supported).unwrap(),
            cpal::BufferSize::Fixed(256)
        ));
        assert!(
            requested_buffer(BufferSize::Fixed(NonZeroU32::new(32).unwrap()), &supported).is_err()
        );
        assert!(
            requested_buffer(
                BufferSize::Fixed(NonZeroU32::new(2048).unwrap()),
                &supported
            )
            .is_err()
        );
    }
}
