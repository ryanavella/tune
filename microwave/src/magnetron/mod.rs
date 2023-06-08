use std::collections::HashMap;

use log::warn;
use magnetron::{
    automation::AutomationSpec, buffer::BufferIndex, creator::Creator, envelope::EnvelopeSpec,
    stage::Stage,
};
use serde::{Deserialize, Serialize};

use self::{
    effects::EffectSpec,
    filter::{Filter, RingModulator},
    oscillator::OscillatorSpec,
    signal::SignalSpec,
    source::StorageAccess,
    waveguide::WaveguideSpec,
};

mod util;

pub mod effects;
pub mod filter;
pub mod oscillator;
pub mod signal;
pub mod source;
pub mod waveguide;

#[derive(Clone, Deserialize, Serialize)]
pub struct TemplateSpec<A> {
    pub name: String,
    pub value: A,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NamedEnvelopeSpec<A> {
    pub name: String,
    #[serde(flatten)]
    pub spec: EnvelopeSpec<A>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WaveformSpec<A> {
    pub name: String,
    pub envelope: String,
    pub stages: Vec<StageSpec<A>>,
}

impl<A: AutomationSpec> WaveformSpec<A> {
    pub fn use_creator(
        &self,
        creator: &Creator<A>,
        envelopes: &HashMap<String, EnvelopeSpec<A>>,
    ) -> Vec<Stage<A::Context>> {
        let internal_stages = self.stages.iter().map(|spec| spec.use_creator(creator));

        let envelope = envelopes.get(&self.envelope);
        if envelope.is_none() {
            warn!("Unknown envelope {}", self.envelope);
        }
        let external_stages = envelope.iter().map(|spec| spec.use_creator(creator));

        internal_stages.chain(external_stages).collect()
    }
}

#[derive(Copy, Clone)]
pub struct WaveformProperties {
    pub pitch_hz: f64,
    pub velocity: f64,
    pub key_pressure: Option<f64>,
    pub off_velocity: Option<f64>,
}

impl WaveformProperties {
    pub fn initial(pitch_hz: f64, velocity: f64) -> Self {
        Self {
            pitch_hz,
            velocity,
            key_pressure: None,
            off_velocity: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum WaveformProperty {
    WaveformPitch,
    WaveformPeriod,
    Velocity,
    KeyPressureSet,
    KeyPressure,
    OffVelocitySet,
    OffVelocity,
}

impl StorageAccess for WaveformProperty {
    type Storage = WaveformProperties;

    fn access(&mut self, storage: &Self::Storage) -> f64 {
        match self {
            WaveformProperty::WaveformPitch => storage.pitch_hz,
            WaveformProperty::WaveformPeriod => storage.pitch_hz.recip(),
            WaveformProperty::Velocity => storage.velocity,
            WaveformProperty::KeyPressureSet => f64::from(u8::from(storage.key_pressure.is_some())),
            WaveformProperty::KeyPressure => storage.key_pressure.unwrap_or_default(),
            WaveformProperty::OffVelocitySet => f64::from(u8::from(storage.off_velocity.is_some())),
            WaveformProperty::OffVelocity => storage.off_velocity.unwrap_or_default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum StageSpec<A> {
    Copy(CopySpec<A>),
    Load(LoadSpec<A>),
    Oscillator(OscillatorSpec<A>),
    Signal(SignalSpec<A>),
    Waveguide(WaveguideSpec<A>),
    Filter(Filter<A>),
    RingModulator(RingModulator<A>),
    Effect(EffectSpec<A>),
}

impl<A: AutomationSpec> StageSpec<A> {
    pub fn use_creator(&self, creator: &Creator<A>) -> Stage<A::Context> {
        match self {
            StageSpec::Copy(spec) => spec.use_creator(creator),
            StageSpec::Load(spec) => spec.use_creator(creator),
            StageSpec::Oscillator(spec) => spec.use_creator(creator),
            StageSpec::Signal(spec) => spec.use_creator(creator),
            StageSpec::Waveguide(spec) => spec.use_creator(creator),
            StageSpec::Filter(spec) => spec.use_creator(creator),
            StageSpec::RingModulator(spec) => spec.use_creator(creator),
            StageSpec::Effect(spec) => spec.use_creator(creator),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CopySpec<A> {
    pub in_buffer: usize,
    #[serde(flatten)]
    pub out_spec: OutSpec<A>,
}

impl<A: AutomationSpec> CopySpec<A> {
    pub fn use_creator(&self, creator: &Creator<A>) -> Stage<A::Context> {
        let (in_buffer, out_buffer) = (
            BufferIndex::Internal(self.in_buffer),
            BufferIndex::Internal(self.out_spec.out_buffer),
        );

        creator.create_stage(&self.out_spec.out_level, move |buffers, out_level| {
            buffers.read_1_write_1(in_buffer, out_buffer, out_level, |sample| sample)
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LoadSpec<A> {
    pub in_buffer: usize,
    #[serde(flatten)]
    pub out_spec: OutSpec<A>,
}

impl<A: AutomationSpec> LoadSpec<A> {
    pub fn use_creator(&self, creator: &Creator<A>) -> Stage<A::Context> {
        let (in_buffer, out_buffer) = (
            BufferIndex::External(self.in_buffer),
            BufferIndex::Internal(self.out_spec.out_buffer),
        );

        creator.create_stage(&self.out_spec.out_level, move |buffers, out_level| {
            buffers.read_1_write_1(in_buffer, out_buffer, out_level, |sample| sample)
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OutSpec<A> {
    pub out_buffer: usize,
    pub out_level: A,
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, f64::consts::TAU, iter};

    use assert_approx_eq::assert_approx_eq;
    use magnetron::{
        automation::AutomationContext, creator::Creator, stage::StageActivity, Magnetron,
    };

    use crate::control::LiveParameter;

    use super::{
        source::{LfSource, LfSourceExpr},
        *,
    };

    const NUM_SAMPLES: usize = 44100;
    const SAMPLE_WIDTH_SECS: f64 = 1.0 / 44100.0;

    #[test]
    fn clear_and_resize_buffers() {
        let mut test = MagnetronTest::new(&[]);

        test.check_audio_out_content(0, |_| 0.0);

        test.process(128, vec![]);
        test.check_audio_out_content(128, |_| 0.0);

        test.process(256, vec![]);
        test.check_audio_out_content(256, |_| 0.0);

        test.process(64, vec![]);
        test.check_audio_out_content(64, |_| 0.0);
    }

    #[test]
    fn empty_spec() {
        let waveform = "[]";

        let mut test = MagnetronTest::new(&[waveform]);

        test.process(NUM_SAMPLES, vec![(440.0, 1.0)]);
        test.check_audio_out_content(NUM_SAMPLES, |_| 0.0);
    }

    #[test]
    fn write_waveform_and_clear() {
        let waveform = r"
- Oscillator:
    kind: Sin
    frequency: WaveformPitch
    modulation: None
    out_buffer: 5
    out_level: 1.0";

        let mut test = MagnetronTest::new(&[waveform]);

        test.process(NUM_SAMPLES, vec![(440.0, 1.0)]);
        test.check_audio_out_content(NUM_SAMPLES, |t| t * (TAU * 440.0 * t).sin());

        test.process(128, vec![]);
        test.check_audio_out_content(128, |_| 0.0);
    }

    #[test]
    fn mix_two_waveforms() {
        let waveform = r"
- Oscillator:
    kind: Sin
    frequency: WaveformPitch
    modulation: None
    out_buffer: 5
    out_level: 1.0";

        let mut test = MagnetronTest::new(&[waveform, waveform]);

        test.process(NUM_SAMPLES, vec![(440.0, 0.7), (660.0, 0.8)]);
        test.check_audio_out_content(NUM_SAMPLES, |t| {
            t * (0.7 * (440.0 * TAU * t).sin() + 0.8 * (660.0 * TAU * t).sin())
        });
    }

    #[test]
    fn apply_optional_phase() {
        let waveform = r"
- Oscillator:
    kind: Sin
    phase: 1.0
    frequency: WaveformPitch
    modulation: None
    out_buffer: 5
    out_level: 1.0";

        let mut test = MagnetronTest::new(&[waveform]);

        test.process(NUM_SAMPLES, vec![(440.0, 1.0)]);
        // 441 Hz because the phase modulates from 0.0 (initial) to 1.0 within 1s (buffer size) leading to one additional oscillation
        test.check_audio_out_content(NUM_SAMPLES, move |t| t * (441.0 * t * TAU).sin());
    }

    #[test]
    fn modulate_by_frequency() {
        let waveform = r"
- Oscillator:
    kind: Sin
    frequency: 330.0
    modulation: None
    out_buffer: 0
    out_level: 440.0
- Oscillator:
    kind: Sin
    frequency: WaveformPitch
    modulation: ByFrequency
    mod_buffer: 0
    out_buffer: 5
    out_level: 1.0";

        let mut test = MagnetronTest::new(&[waveform]);

        test.process(NUM_SAMPLES, vec![(550.0, 1.0)]);
        test.check_audio_out_content(NUM_SAMPLES, {
            let mut mod_phase = 0.0;
            move |t| {
                let signal = ((550.0 * t + mod_phase) * TAU).sin();
                mod_phase += (330.0 * TAU * t).sin() * 440.0 * SAMPLE_WIDTH_SECS;
                t * signal
            }
        });
    }

    #[test]
    fn modulate_by_phase() {
        let waveform = r"
- Oscillator:
    kind: Sin
    frequency: 330.0
    modulation: None
    out_buffer: 0
    out_level: 0.44
- Oscillator:
    kind: Sin
    frequency: WaveformPitch
    modulation: ByPhase
    mod_buffer: 0
    out_buffer: 5
    out_level: 1.0";

        let mut test = MagnetronTest::new(&[waveform]);

        test.process(NUM_SAMPLES, vec![(550.0, 1.0)]);
        test.check_audio_out_content(NUM_SAMPLES, |t| {
            t * ((550.0 * t + (330.0 * TAU * t).sin() * 0.44) * TAU).sin()
        });
    }

    #[test]
    fn ring_modulation() {
        let waveform = r"
- Oscillator:
    kind: Sin
    frequency: WaveformPitch
    modulation: None
    out_buffer: 0
    out_level: 1.0
- Oscillator:
    kind: Sin
    frequency:
        Mul: [1.5, WaveformPitch]
    modulation: None
    out_buffer: 1
    out_level: 1.0
- RingModulator:
    in_buffers: [0, 1]
    out_buffer: 5
    out_level: 1.0";

        let mut test = MagnetronTest::new(&[waveform]);

        test.process(NUM_SAMPLES, vec![(440.0, 1.0)]);
        test.check_audio_out_content(NUM_SAMPLES, |t| {
            t * (440.0 * t * TAU).sin() * (660.0 * t * TAU).sin()
        });
    }

    #[test]
    fn evaluate_envelope_varying_attack_time() {
        let waveform = r"
- Oscillator:
    kind: Sin
    frequency: WaveformPitch
    modulation: None
    out_buffer: 5
    out_level: 1.0";

        let mut test = MagnetronTest::new_with_envelope(
            &[waveform],
            EnvelopeSpec {
                fadeout: LfSource::Value(0.0),
                attack_time: LfSource::template("Velocity"),
                decay_rate: LfSource::Value(1.0),
                release_time: LfSource::Value(1.0),
                in_buffer: 5,
                out_buffers: (6, 7),
                out_levels: (LfSource::Value(1.0), LfSource::Value(1.0)),
            },
        );

        // attack part 1
        test.process(NUM_SAMPLES, vec![(440.0, 3.0)]);
        test.check_audio_out_content(NUM_SAMPLES, |t| 1.0 / 3.0 * t * (TAU * 440.0 * t).sin());

        // attack part 2
        test.process(NUM_SAMPLES, vec![(440.0, 3.0 / 2.0)]);
        test.check_audio_out_content(NUM_SAMPLES, |t| {
            (1.0 / 3.0 + 2.0 / 3.0 * t) * (TAU * 440.0 * t).sin()
        });

        // decay part
        test.process(NUM_SAMPLES, vec![(440.0, 1.0)]);
        test.check_audio_out_content(NUM_SAMPLES, |t| {
            (1.0 - 1.0 / 2.0 * t) * (TAU * 440.0 * t).sin()
        });
    }

    #[test]
    fn evaluate_envelope_varying_decay_time() {
        let waveform = r"
- Oscillator:
    kind: Sin
    frequency: WaveformPitch
    modulation: None
    out_buffer: 5
    out_level: 1.0";

        let mut test = MagnetronTest::new_with_envelope(
            &[waveform],
            EnvelopeSpec {
                fadeout: LfSource::Value(0.0),
                attack_time: LfSource::Value(1.0),
                decay_rate: LfSource::template("Velocity"),
                release_time: LfSource::Value(1.0),
                in_buffer: 5,
                out_buffers: (6, 7),
                out_levels: (LfSource::Value(1.0), LfSource::Value(1.0)),
            },
        );

        // attack part
        test.process(NUM_SAMPLES, vec![(440.0, 1.0)]);
        test.check_audio_out_content(NUM_SAMPLES, |t| t * (TAU * 440.0 * t).sin());

        // decay part 1
        test.process(NUM_SAMPLES, vec![(440.0, 1.0)]);
        test.check_audio_out_content(NUM_SAMPLES, |t| {
            (1.0 - 1.0 / 2.0 * t) * (TAU * 440.0 * t).sin()
        });

        // decay part 2
        test.process(NUM_SAMPLES, vec![(440.0, 2.0)]);
        test.check_audio_out_content(NUM_SAMPLES, |t| {
            (1.0 / 2.0 - 3.0 / 8.0 * t) * (TAU * 440.0 * t).sin()
        });
    }

    #[test]
    fn evaluate_envelope_varying_fadeout() {
        let waveform = r"
- Oscillator:
    kind: Sin
    frequency: WaveformPitch
    modulation: None
    out_buffer: 5
    out_level: 1.0";

        let mut test = MagnetronTest::new_with_envelope(
            &[waveform],
            EnvelopeSpec {
                fadeout: LfSource::template("Velocity"),
                attack_time: LfSource::Value(1.0),
                decay_rate: LfSource::Value(0.0),
                release_time: LfSource::Value(3.0),
                in_buffer: 5,
                out_buffers: (6, 7),
                out_levels: (LfSource::Value(1.0), LfSource::Value(1.0)),
            },
        );

        // attack part
        assert_eq!(
            test.process(NUM_SAMPLES, vec![(440.0, 0.0)]),
            StageActivity::External
        );
        test.check_audio_out_content(NUM_SAMPLES, |t| t * (TAU * 440.0 * t).sin());

        // sustain part
        assert_eq!(
            test.process(NUM_SAMPLES, vec![(440.0, 0.0)]),
            StageActivity::External
        );
        test.check_audio_out_content(NUM_SAMPLES, |t| (TAU * 440.0 * t).sin());

        // release part 1
        assert_eq!(
            test.process(NUM_SAMPLES, vec![(440.0, 1.0)]),
            StageActivity::External
        );
        test.check_audio_out_content(NUM_SAMPLES, |t| {
            (1.0 - 1.0 / 3.0 * t) * (TAU * 440.0 * t).sin()
        });

        // release part 1
        assert_eq!(
            test.process(NUM_SAMPLES, vec![(440.0, 2.0)]),
            StageActivity::Internal
        );
        test.check_audio_out_content(NUM_SAMPLES, |t| {
            (2.0 / 3.0 - 2.0 / 3.0 * t) * (TAU * 440.0 * t).sin()
        });
    }

    struct MagnetronTest {
        magnetron: Magnetron,
        stage: Stage<Vec<(f64, f64)>>,
        result_l: Vec<f64>,
        result_r: Vec<f64>,
    }

    impl MagnetronTest {
        fn new(waveform_specs: &[&str]) -> Self {
            Self::new_with_envelope(
                waveform_specs,
                EnvelopeSpec {
                    fadeout: LfSource::Value(0.0),
                    attack_time: LfSource::Value(0.0),
                    decay_rate: LfSource::Value(0.0),
                    release_time: LfSource::Value(0.0),
                    in_buffer: 5,
                    out_buffers: (6, 7),
                    out_levels: (
                        LfSource::template("Velocity"),
                        LfSource::template("Velocity"),
                    ),
                },
            )
        }

        fn new_with_envelope(
            waveform_specs: &[&str],
            envelope_spec: EnvelopeSpec<LfSource<WaveformProperty, LiveParameter>>,
        ) -> Self {
            let creator = Creator::new(HashMap::from([
                (
                    "WaveformPitch".to_owned(),
                    LfSourceExpr::Property(WaveformProperty::WaveformPitch).wrap(),
                ),
                (
                    "Velocity".to_owned(),
                    LfSourceExpr::Property(WaveformProperty::Velocity).wrap(),
                ),
            ]));

            let envelopes = HashMap::from([("test envelope".to_owned(), envelope_spec)]);
            let mut waveforms: Vec<_> = waveform_specs
                .iter()
                .map(|spec| {
                    WaveformSpec {
                        name: String::new(),
                        envelope: "test envelope".to_owned(),
                        stages: serde_yaml::from_str(spec).unwrap(),
                    }
                    .use_creator(&creator, &envelopes)
                })
                .collect();

            let mut magnetron = create_magnetron();

            let stage = Stage::new(
                move |buffers, context: &AutomationContext<Vec<(f64, f64)>>| {
                    iter::zip(context.payload, &mut waveforms)
                        .map(|((pitch_hz, velocity), waveform)| {
                            magnetron.prepare_nested(buffers).process(
                                &(
                                    WaveformProperties::initial(*pitch_hz, *velocity),
                                    Default::default(),
                                ),
                                waveform,
                            )
                        })
                        .max()
                        .unwrap_or_default()
                },
            );

            Self {
                magnetron: create_magnetron(),
                stage,
                result_l: Vec::new(),
                result_r: Vec::new(),
            }
        }

        fn process(&mut self, num_samples: usize, render_passes: Vec<(f64, f64)>) -> StageActivity {
            let buffers = &mut self.magnetron.prepare(num_samples, false);
            let activity = buffers.process(&render_passes, [&mut self.stage]);
            self.result_l.clear();
            self.result_r.clear();
            self.result_l.extend(buffers.read(BufferIndex::Internal(6)));
            self.result_r.extend(buffers.read(BufferIndex::Internal(7)));
            activity
        }

        fn check_audio_out_content(&self, num_samples: usize, mut f: impl FnMut(f64) -> f64) {
            check_sampled_signal(&self.result_l, num_samples, &mut f);
            check_sampled_signal(&self.result_r, num_samples, &mut f);
        }
    }

    fn create_magnetron() -> Magnetron {
        Magnetron::new(SAMPLE_WIDTH_SECS, 8, 100000)
    }

    fn check_sampled_signal(buffer: &[f64], num_samples: usize, mut f: impl FnMut(f64) -> f64) {
        assert_eq!(buffer.len(), num_samples);

        let mut time = 0.0;
        for sample in buffer {
            assert_approx_eq!(sample, f(time));
            time += SAMPLE_WIDTH_SECS;
        }
    }
}
