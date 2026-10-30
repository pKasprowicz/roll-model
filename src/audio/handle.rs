//! The two ends of the engine: the [`EngineHandle`] that the threads which schedule sounds hold, and
//! the [`Callback`] that the audio backend runs for every buffer. Between them go commands through a
//! wait-free queue and the clock estimate through [`Shared`]. Neither end knows about a sound device,
//! so the whole path is tested by simulating callbacks.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use rtrb::{Consumer, Producer, RingBuffer};

use super::clock::{Estimator, Shared};
use super::mixer::{Command, Mixer, Trigger};

/// Commands that can wait for the callback.
pub const QUEUE: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayError {
    /// The callback has not run yet, so there are no frames to tell the start by.
    NotReady,
    /// The queue is full: the callback is not keeping up, or the sound was queued far too early.
    QueueFull,
}

/// Used from any number of threads; clones share one engine.
#[derive(Clone)]
pub struct EngineHandle {
    shared: Arc<Shared>,
    // The queue has one producer; the scheduler threads take turns. Only the callback side must not lock.
    queue: Arc<Mutex<Producer<Command>>>,
}

impl EngineHandle {
    pub fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }

    /// Plays `sample` so that it starts at `at`. `group` and `chokes` are the choke groups of
    /// [`Trigger`]. The call does not wait: the sound is in the queue when it returns.
    pub fn play(&self, run: u32, at: Instant, sample: &Arc<[f32]>, gain: f32, group: u8, chokes: u32) -> Result<(), PlayError> {
        let start_frame = self.shared.frame_at(at).ok_or(PlayError::NotReady)?;
        self.send(Command::Play(Trigger { run, start_frame, sample: sample.clone(), gain, group, chokes }))
    }

    /// Ends the sounds of `run`: the waiting ones are dropped, the playing ones fade out.
    pub fn cancel_run(&self, run: u32) -> Result<(), PlayError> {
        self.send(Command::CancelRun(run))
    }

    fn send(&self, command: Command) -> Result<(), PlayError> {
        let mut queue = self.queue.lock().map_err(|_| PlayError::QueueFull)?;
        queue.push(command).map_err(|_| PlayError::QueueFull)
    }
}

/// What the audio backend runs in its callback.
pub struct Callback {
    shared: Arc<Shared>,
    queue: Consumer<Command>,
    mixer: Mixer,
    estimator: Estimator,
}

impl Callback {
    /// Fills `out` (interleaved, `channels` channels). `wall` is how many seconds after the engine's
    /// origin the callback runs; the backend reads the clock as early in the callback as it can.
    pub fn render(&mut self, out: &mut [f32], channels: usize, wall: f64) {
        let intercept = self.estimator.observe(wall, self.mixer.frame());
        // Publish before taking commands, so a sound sent now is placed with this estimate.
        self.shared.publish(intercept, self.mixer.counters());
        while let Ok(command) = self.queue.pop() {
            self.mixer.apply(command);
        }
        self.mixer.render(out, channels);
    }

    pub fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }
}

/// A new engine for a stream of `rate` frames per second; `origin` is the instant the callback's
/// `wall` seconds count from.
pub fn channel(rate: u32, origin: Instant) -> (EngineHandle, Callback) {
    let shared = Arc::new(Shared::new(origin, rate));
    let (producer, consumer) = RingBuffer::new(QUEUE);
    let handle = EngineHandle { shared: shared.clone(), queue: Arc::new(Mutex::new(producer)) };
    let callback = Callback { shared, queue: consumer, mixer: Mixer::new(), estimator: Estimator::new(rate) };
    (handle, callback)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const RATE: u32 = 48_000;
    const BLOCK: usize = 480;

    /// A small deterministic noise in 0..1.
    fn noise(i: u64) -> f64 {
        let x = i.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((x >> 33) as f64) / (1u64 << 31) as f64
    }

    fn impulse(frames: usize) -> Arc<[f32]> {
        vec![1.0; frames].into()
    }

    #[test]
    fn nothing_can_be_played_before_the_first_callback() {
        let origin = Instant::now();
        let (handle, mut callback) = channel(RATE, origin);
        let s = impulse(4);
        assert_eq!(handle.play(1, origin, &s, 1.0, 255, 0), Err(PlayError::NotReady));
        callback.render(&mut vec![0.0; BLOCK], 1, 0.0);
        assert_eq!(handle.play(1, origin, &s, 1.0, 255, 0), Ok(()));
    }

    #[test]
    fn a_full_queue_is_reported() {
        let origin = Instant::now();
        let (handle, mut callback) = channel(RATE, origin);
        callback.render(&mut vec![0.0; BLOCK], 1, 0.0);
        let s = impulse(1);
        for _ in 0..QUEUE {
            assert_eq!(handle.play(1, origin + Duration::from_secs(100), &s, 1.0, 255, 0), Ok(()));
        }
        assert_eq!(handle.play(1, origin + Duration::from_secs(100), &s, 1.0, 255, 0), Err(PlayError::QueueFull));
        callback.render(&mut vec![0.0; BLOCK], 1, 0.01); // the callback takes the queue; only the pool is limited
        assert_eq!(handle.play(1, origin + Duration::from_secs(100), &s, 1.0, 255, 0), Ok(()));
    }

    #[test]
    fn cancelling_a_run_reaches_the_mixer() {
        let origin = Instant::now();
        let (handle, mut callback) = channel(RATE, origin);
        callback.render(&mut vec![0.0; BLOCK], 1, 0.0);
        handle.play(7, origin + Duration::from_millis(100), &impulse(4), 1.0, 255, 0).unwrap();
        handle.cancel_run(7).unwrap();
        let mut out = vec![0.0; BLOCK * 20];
        callback.render(&mut out, 1, 0.01);
        assert!(out.iter().all(|s| *s == 0.0), "the cancelled sound never played");
    }

    /// The property the engine is for: sounds scheduled on the program's clock start at the frame that
    /// the sound card renders at that moment, however late the callbacks run and however the sound
    /// card's clock drifts, so they do not move with the callback period.
    #[test]
    fn sounds_start_where_the_program_clock_says_whatever_the_callbacks_do() {
        for drift in [0.0, -80e-6, 80e-6] {
            let origin = Instant::now();
            let (handle, mut callback) = channel(RATE, origin);
            let frame_secs = 1.0 / (RATE as f64 * (1.0 + drift)); // how long a frame really takes
            let w0 = 3.0; // the time of the stream's first frame, seconds after the origin
            let (period, lookahead) = (0.0625, 0.030);

            // Sounds every 62.5 ms from 2 s into the stream, as a metronome at 240 BPM in sixteenths.
            let wanted: Vec<f64> = (0..200).map(|k| w0 + 2.0 + k as f64 * period).collect();
            let mut queued = 0;
            let mut played: Vec<f32> = Vec::new();
            for i in 0..(8.0 / 0.01) as u64 {
                // The callback of block `i` runs a little late, now and then a lot.
                let wall = w0 + (i as usize * BLOCK) as f64 * frame_secs + noise(i) * 0.004 + if i % 41 == 0 { 0.009 } else { 0.0 };
                // The scheduler queues what falls within its look-ahead before this callback.
                while queued < wanted.len() && wanted[queued] < wall + lookahead {
                    let at = origin + Duration::from_secs_f64(wanted[queued]);
                    if handle.play(1, at, &impulse(8), 1.0, 255, 0).is_ok() {
                        queued += 1;
                    } else {
                        break; // not ready yet: the first callback has not run
                    }
                }
                let mut out = vec![0.0; BLOCK];
                callback.render(&mut out, 1, wall);
                played.extend(out);
            }

            // Where each sound really started, against where the sound card's clock says it should.
            let mut starts: Vec<usize> = Vec::new();
            for (n, s) in played.iter().enumerate() {
                if *s > 0.0 && (n == 0 || played[n - 1] == 0.0) {
                    starts.push(n);
                }
            }
            assert!(starts.len() >= 90, "drift {drift}: only {} sounds played", starts.len());
            let errors_ms: Vec<f64> = starts
                .iter()
                .zip(&wanted)
                .map(|(frame, t)| (*frame as f64 * frame_secs + w0 - t) * 1000.0)
                .collect();
            let worst = errors_ms.iter().fold(0.0f64, |m, e| m.max(e.abs()));
            println!("drift {drift:+.0e}: {} sounds, worst onset error {worst:.3} ms", starts.len());
            assert!(worst < 0.5, "drift {drift}: worst onset error {worst:.3} ms of {} sounds", starts.len());
            assert_eq!(callback.shared().counters().late, 0, "nothing was queued too late");
        }
    }
}
