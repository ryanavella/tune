use serde::{Deserialize, Serialize};
use tune::pitch::Pitch;

use super::{
    control::Controller,
    envelope::EnvelopeType,
    filter::{Filter, RingModulator},
    oscillator::Oscillator,
    source::LfSource,
    Magnetron, WaveformControl,
};

#[derive(Deserialize, Serialize)]
pub struct WaveformSpec<C> {
    pub name: String,
    pub envelope_type: EnvelopeType,
    pub stages: Vec<StageSpec<C>>,
}

impl<C: Controller> WaveformSpec<C> {
    pub fn create_waveform(
        &self,
        pitch: Pitch,
        amplitude: f64,
        envelope_type: Option<EnvelopeType>,
    ) -> Waveform<C::Storage> {
        let envelope_type = envelope_type.unwrap_or(self.envelope_type);
        Waveform {
            envelope_type,
            stages: self.stages.iter().map(StageSpec::create_stage).collect(),
            pitch,
            total_time_in_s: 0.0,
            curr_amplitude: amplitude,
            amplitude_change_rate_hz: -amplitude * envelope_type.decay_rate_hz(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn envelope_type(&self) -> EnvelopeType {
        self.envelope_type
    }
}

#[derive(Deserialize, Serialize)]
pub enum StageSpec<K> {
    Oscillator(Oscillator<K>),
    Filter(Filter<K>),
    RingModulator(RingModulator<K>),
}

impl<C: Controller> StageSpec<C> {
    fn create_stage(&self) -> Stage<C::Storage> {
        match self {
            StageSpec::Oscillator(oscillation) => oscillation.create_stage(),
            StageSpec::Filter(filter) => filter.create_stage(),
            StageSpec::RingModulator(ring_modulator) => ring_modulator.create_stage(),
        }
    }
}

pub struct Waveform<S> {
    pub envelope_type: EnvelopeType,
    pub stages: Vec<Stage<S>>,
    pub pitch: Pitch,
    pub total_time_in_s: f64,
    pub curr_amplitude: f64,
    pub amplitude_change_rate_hz: f64,
}

impl<S> Waveform<S> {
    pub fn pitch(&self) -> Pitch {
        self.pitch
    }

    pub fn set_pitch(&mut self, pitch: Pitch) {
        self.pitch = pitch;
    }

    pub fn set_fade(&mut self, decay_amount: f64) {
        let interpolation = (1.0 - decay_amount) * self.envelope_type.release_rate_hz()
            + decay_amount * self.envelope_type.decay_rate_hz();
        self.amplitude_change_rate_hz = -self.curr_amplitude * interpolation;
    }

    pub fn amplitude(&self) -> f64 {
        self.curr_amplitude
    }
}

pub type Stage<S> = Box<dyn FnMut(&mut Magnetron, &WaveformControl<S>) + Send>;

#[derive(Clone, Deserialize, Serialize)]
pub enum Source {
    AudioIn,
    Buffer0,
    Buffer1,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Destination<C> {
    pub buffer: OutBuffer,
    pub intensity: LfSource<C>,
}

#[derive(Clone, Deserialize, Serialize)]
pub enum OutBuffer {
    Buffer0,
    Buffer1,
    AudioOut,
}