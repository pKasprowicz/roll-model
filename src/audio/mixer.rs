//! The mixing core of the audio engine. It is rendered from the audio callback, so it never
//! allocates, locks or frees: the voice pool and the list of sounds waiting for their frame have a
//! fixed capacity (a sound that does not fit is dropped and counted), and the samples are shared
//! `Arc`s that something else keeps alive, so a voice that ends only lowers a count.
//!
//! A sound is a [`Trigger`] with the frame it starts at. The mixer counts frames from the first one
//! it renders; a trigger whose frame lies inside the block being rendered starts at exactly that
//! frame, whatever the size of the block.

use std::sync::Arc;

use rtrb::Producer;

/// Sounds that can play at once; one more is dropped.
pub const MAX_VOICES: usize = 64;
/// Sounds that can wait for their frame; one more is dropped.
pub const MAX_PENDING: usize = 256;
/// A choked or cancelled voice fades out over this many frames, so it does not click.
pub const FADE_FRAMES: u32 = 256;
/// The `group` of a sound that nothing can choke.
pub const NO_GROUP: u8 = u8::MAX;

/// A sound to start at a frame of the stream.
#[derive(Clone)]
pub struct Trigger {
    /// The run this sound belongs to (a Listen, a take, one audition): [`Command::CancelRun`] ends all of them.
    pub run: u32,
    /// The frame the first sample is rendered at.
    pub start_frame: u64,
    /// Mono samples at the stream's rate.
    pub sample: Arc<[f32]>,
    pub gain: f32,
    /// Which sounds can choke this one: a bit number below 32, or [`NO_GROUP`].
    pub group: u8,
    /// The groups this sound chokes when it starts (bit `n` = group `n`).
    pub chokes: u32,
}

/// What the callback is told to do.
#[derive(Clone)]
pub enum Command {
    Play(Trigger),
    /// Drops the waiting sounds of a run and fades out the ones that are playing.
    CancelRun(u32),
}

/// What went wrong since the start, for a readout (a rising number means the engine is overloaded
/// or a sound was queued too late).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    /// Sounds whose frame had already passed when the block started; they started at its first frame.
    pub late: u64,
    /// The most frames any of them was late by.
    pub worst_late: u64,
    /// Sounds dropped because [`MAX_PENDING`] were waiting.
    pub dropped_pending: u64,
    /// Sounds dropped because [`MAX_VOICES`] were playing.
    pub dropped_voices: u64,
}

struct Voice {
    run: u32,
    sample: Arc<[f32]>,
    pos: usize,
    gain: f32,
    group: u8,
    /// Frames of fade-out left once choked; `None` while the voice plays normally.
    fade_left: Option<u32>,
}

impl Voice {
    fn fade(&mut self) {
        if self.fade_left.is_none() {
            self.fade_left = Some(FADE_FRAMES);
        }
    }
}

pub struct Mixer {
    voices: Vec<Voice>,
    pending: Vec<Trigger>,
    /// The first frame of the next block.
    frame: u64,
    /// The earliest `start_frame` of the waiting sounds; `u64::MAX` when none wait.
    next_due: u64,
    counters: Counters,
    /// Where a sample goes when its sound is over, so that the last reference to it is let go of
    /// outside the audio callback (freeing memory there can take long enough to be heard).
    graveyard: Option<Producer<Arc<[f32]>>>,
}

impl Default for Mixer {
    fn default() -> Self {
        Mixer::new()
    }
}

impl Mixer {
    pub fn new() -> Mixer {
        Mixer { voices: Vec::with_capacity(MAX_VOICES), pending: Vec::with_capacity(MAX_PENDING), frame: 0, next_due: u64::MAX, counters: Counters::default(), graveyard: None }
    }

    /// A mixer that hands the samples of finished sounds to `graveyard`.
    pub fn with_graveyard(mut self, graveyard: Producer<Arc<[f32]>>) -> Mixer {
        self.graveyard = Some(graveyard);
        self
    }

    fn bury(&mut self, sample: Arc<[f32]>) {
        if let Some(g) = self.graveyard.as_mut() {
            // A full graveyard hands the sample back, and it is let go of here after all.
            let _ = g.push(sample);
        }
    }

    /// The first frame of the next block to render.
    pub fn frame(&self) -> u64 {
        self.frame
    }

    pub fn counters(&self) -> Counters {
        self.counters
    }

    #[cfg(test)]
    pub fn playing(&self) -> usize {
        self.voices.len()
    }

    #[cfg(test)]
    pub fn waiting(&self) -> usize {
        self.pending.len()
    }

    pub fn apply(&mut self, command: Command) {
        match command {
            Command::Play(trigger) => {
                if self.pending.len() >= MAX_PENDING {
                    self.counters.dropped_pending += 1;
                    return;
                }
                self.next_due = self.next_due.min(trigger.start_frame);
                self.pending.push(trigger);
            }
            Command::CancelRun(run) => {
                let mut i = 0;
                while i < self.pending.len() {
                    if self.pending[i].run == run {
                        let t = self.pending.remove(i);
                        self.bury(t.sample);
                    } else {
                        i += 1;
                    }
                }
                self.next_due = self.pending.iter().map(|t| t.start_frame).min().unwrap_or(u64::MAX);
                for v in self.voices.iter_mut().filter(|v| v.run == run) {
                    v.fade();
                }
            }
        }
    }

    /// Mixes into `out`, interleaved with `channels` channels (the mix is mono and goes to every
    /// channel), and moves on by the frames that fit. A trailing partial frame is left alone.
    pub fn render(&mut self, out: &mut [f32], channels: usize) {
        let channels = channels.max(1);
        let block_start = self.frame;
        let mut frames = 0u64;
        for frame_out in out.chunks_exact_mut(channels) {
            let now = block_start + frames;
            if self.next_due <= now {
                self.start_due(now, block_start);
            }
            let mix = self.mix_frame().clamp(-1.0, 1.0);
            frame_out.fill(mix);
            frames += 1;
        }
        self.frame = block_start + frames;
    }

    /// Starts the waiting sounds whose frame has come, in the order they were queued.
    fn start_due(&mut self, now: u64, block_start: u64) {
        let mut next = u64::MAX;
        let mut i = 0;
        while i < self.pending.len() {
            if self.pending[i].start_frame > now {
                next = next.min(self.pending[i].start_frame);
                i += 1;
                continue;
            }
            // In the order they were queued, so sounds that start together choke each other predictably.
            let t = self.pending.remove(i);
            if t.start_frame < block_start {
                self.counters.late += 1;
                self.counters.worst_late = self.counters.worst_late.max(block_start - t.start_frame);
            }
            for v in self.voices.iter_mut() {
                if v.group < 32 && t.chokes & (1 << v.group) != 0 {
                    v.fade();
                }
            }
            if self.voices.len() < MAX_VOICES {
                self.voices.push(Voice { run: t.run, sample: t.sample, pos: 0, gain: t.gain, group: t.group, fade_left: None });
            } else {
                self.counters.dropped_voices += 1;
                self.bury(t.sample);
            }
        }
        self.next_due = next;
    }

    fn mix_frame(&mut self) -> f32 {
        let mut mix = 0.0f32;
        let mut i = 0;
        while i < self.voices.len() {
            let v = &mut self.voices[i];
            let ramp = match v.fade_left {
                Some(0) => {
                    let v = self.voices.swap_remove(i);
                    self.bury(v.sample);
                    continue;
                }
                Some(left) => {
                    v.fade_left = Some(left - 1);
                    (left - 1) as f32 / FADE_FRAMES as f32
                }
                None => 1.0,
            };
            match v.sample.get(v.pos) {
                Some(s) => {
                    mix += s * v.gain * ramp;
                    v.pos += 1;
                    i += 1;
                }
                None => {
                    let v = self.voices.swap_remove(i);
                    self.bury(v.sample);
                }
            }
        }
        mix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constant(value: f32, frames: usize) -> Arc<[f32]> {
        vec![value; frames].into()
    }

    fn trigger(run: u32, start_frame: u64, sample: &Arc<[f32]>, gain: f32) -> Command {
        Command::Play(Trigger { run, start_frame, sample: sample.clone(), gain, group: NO_GROUP, chokes: 0 })
    }

    fn render_mono(m: &mut Mixer, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0; frames];
        m.render(&mut out, 1);
        out
    }

    #[test]
    fn a_sound_starts_at_exactly_its_frame() {
        let mut m = Mixer::new();
        m.apply(trigger(1, 1000, &constant(0.5, 10), 1.0));
        let out = render_mono(&mut m, 2048);
        assert_eq!(out[999], 0.0);
        assert_eq!(out[1000], 0.5);
        assert_eq!(out[1009], 0.5);
        assert_eq!(out[1010], 0.0, "and ends after its samples");
        assert_eq!((m.playing(), m.waiting(), m.counters().late), (0, 0, 0));
    }

    #[test]
    fn the_size_of_the_blocks_changes_nothing() {
        let sample: Arc<[f32]> = (0..700).map(|i| (i as f32 * 0.05).sin()).collect::<Vec<_>>().into();
        let queue = |m: &mut Mixer| {
            for (k, start) in [100u64, 450, 451, 2500, 4000].into_iter().enumerate() {
                m.apply(trigger(1, start, &sample, 0.3 + 0.1 * k as f32));
            }
        };
        let mut whole = Mixer::new();
        queue(&mut whole);
        let expected = render_mono(&mut whole, 6000);

        let mut pieces = Mixer::new();
        queue(&mut pieces);
        let mut got = Vec::new();
        for size in [480, 1, 333, 1000, 7, 2000, 2179] {
            got.extend(render_mono(&mut pieces, size));
        }
        assert_eq!(got, expected);
        assert_eq!(pieces.frame(), 6000);
    }

    #[test]
    fn the_mix_goes_to_every_channel_and_the_gain_applies() {
        let mut m = Mixer::new();
        m.apply(trigger(1, 2, &constant(0.5, 4), 0.5));
        let mut out = vec![9.0; 6 * 3];
        m.render(&mut out, 3);
        let frames: Vec<&[f32]> = out.chunks(3).collect();
        assert_eq!(frames[1], [0.0; 3]);
        assert_eq!(frames[2], [0.25; 3]);
        assert_eq!(frames[5], [0.25; 3]);
    }

    #[test]
    fn a_trailing_partial_frame_is_left_alone() {
        let mut m = Mixer::new();
        let mut out = vec![7.0; 5];
        m.render(&mut out, 2);
        assert_eq!(m.frame(), 2);
        assert_eq!(out[4], 7.0);
    }

    #[test]
    fn a_late_sound_starts_at_the_first_frame_of_the_block_and_is_counted() {
        let mut m = Mixer::new();
        render_mono(&mut m, 100);
        m.apply(trigger(1, 40, &constant(1.0, 5), 0.5));
        let out = render_mono(&mut m, 20);
        assert_eq!(out[0], 0.5);
        assert_eq!(out[5], 0.0);
        assert_eq!(m.counters().late, 1);
    }

    #[test]
    fn simultaneous_sounds_add_and_the_mix_is_clamped() {
        let mut m = Mixer::new();
        for _ in 0..3 {
            m.apply(trigger(1, 0, &constant(0.6, 4), 1.0));
        }
        assert_eq!(render_mono(&mut m, 5), [1.0, 1.0, 1.0, 1.0, 0.0]);
    }

    #[test]
    fn a_choked_voice_fades_out_and_ends() {
        let mut m = Mixer::new();
        let open = Command::Play(Trigger { run: 1, start_frame: 0, sample: constant(1.0, 10_000), gain: 1.0, group: 4, chokes: 0 });
        let closed = Command::Play(Trigger { run: 1, start_frame: 1000, sample: constant(0.0, 1), gain: 1.0, group: NO_GROUP, chokes: 1 << 4 });
        m.apply(open);
        m.apply(closed);
        let out = render_mono(&mut m, 2000);
        assert_eq!(out[999], 1.0, "untouched until the choke");
        assert!(out[1000] < 1.0 && out[1000] > 0.99, "the fade starts at the choke frame: {}", out[1000]);
        assert!(out[1100] < out[1000] && out[1100] > 0.4, "{}", out[1100]);
        assert!(out[1000..1000 + FADE_FRAMES as usize].windows(2).all(|w| w[1] <= w[0]), "monotonic fade");
        assert_eq!(out[1000 + FADE_FRAMES as usize], 0.0);
        assert_eq!(m.playing(), 0);
    }

    #[test]
    fn a_choke_leaves_other_groups_alone() {
        let mut m = Mixer::new();
        m.apply(Command::Play(Trigger { run: 1, start_frame: 0, sample: constant(0.5, 3000), gain: 1.0, group: 7, chokes: 0 }));
        m.apply(Command::Play(Trigger { run: 1, start_frame: 10, sample: constant(0.0, 1), gain: 1.0, group: NO_GROUP, chokes: 1 << 4 }));
        let out = render_mono(&mut m, 2000);
        assert_eq!(out[1500], 0.5);
    }

    #[test]
    fn cancelling_a_run_drops_its_waiting_sounds_and_fades_its_voices() {
        let mut m = Mixer::new();
        m.apply(trigger(1, 0, &constant(1.0, 5000), 1.0));
        m.apply(trigger(1, 3000, &constant(1.0, 100), 1.0));
        m.apply(trigger(2, 0, &constant(0.25, 5000), 1.0));
        m.apply(trigger(2, 3000, &constant(0.25, 100), 1.0));
        render_mono(&mut m, 500);
        m.apply(Command::CancelRun(1));
        assert_eq!(m.waiting(), 1, "only the other run's sound still waits");
        let out = render_mono(&mut m, 3500);
        assert_eq!(out[FADE_FRAMES as usize], 0.25, "run 1 has faded out, run 2 plays on");
        assert!(out[10] > 0.25 && out[10] < 1.25, "run 1 is fading: {}", out[10]);
        assert_eq!(out[3000 - 500 - 1], 0.25, "run 2's long sound alone before the later one");
        assert_eq!(out[3000 - 500 + 50], 0.5, "run 2's later sound still starts, on top of it");
    }

    #[test]
    fn a_full_pool_drops_the_new_sound_and_counts_it_without_growing() {
        let mut m = Mixer::new();
        let (voice_cap, pending_cap) = (m.voices.capacity(), m.pending.capacity());
        for _ in 0..MAX_VOICES + 6 {
            m.apply(trigger(1, 0, &constant(0.01, 1000), 1.0));
        }
        render_mono(&mut m, 10);
        assert_eq!(m.playing(), MAX_VOICES);
        assert_eq!(m.counters().dropped_voices, 6);
        assert_eq!((m.voices.capacity(), m.pending.capacity()), (voice_cap, pending_cap));
    }

    #[test]
    fn too_many_waiting_sounds_are_dropped_and_counted() {
        let mut m = Mixer::new();
        let pending_cap = m.pending.capacity();
        for k in 0..MAX_PENDING + 44 {
            m.apply(trigger(1, 1_000_000 + k as u64, &constant(0.1, 1), 1.0));
        }
        assert_eq!(m.waiting(), MAX_PENDING);
        assert_eq!(m.counters().dropped_pending, 44);
        assert_eq!(m.pending.capacity(), pending_cap);
        assert!(render_mono(&mut m, 100).iter().all(|s| *s == 0.0));
    }

    #[test]
    fn a_long_busy_run_never_grows_the_buffers() {
        let mut m = Mixer::new();
        let (voice_cap, pending_cap) = (m.voices.capacity(), m.pending.capacity());
        let sample = constant(0.05, 700);
        for k in 0..2000u64 {
            m.apply(trigger((k % 3) as u32, 480 * k / 2 + 37, &sample, 0.5));
            if k % 50 == 49 {
                m.apply(Command::CancelRun((k % 3) as u32));
            }
            render_mono(&mut m, 240);
        }
        assert_eq!((m.voices.capacity(), m.pending.capacity()), (voice_cap, pending_cap));
    }
}
