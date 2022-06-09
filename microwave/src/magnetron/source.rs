use std::{
    fmt,
    marker::PhantomData,
    ops::{Add, Mul},
};

use serde::{
    de::{self, value::MapAccessDeserializer, IntoDeserializer, Visitor},
    Deserialize, Deserializer, Serialize,
};

use super::{
    control::Controller,
    functions,
    oscillator::OscillatorKind,
    waveform::{Creator, Spec},
    AutomatedValue, AutomationContext,
};

pub struct Automation<S> {
    automation_fn: Box<dyn FnMut(&AutomationContext<S>) -> f64 + Send>,
}

impl<S> Automation<S> {
    pub fn new(automation_fn: impl FnMut(&AutomationContext<S>) -> f64 + Send + 'static) -> Self {
        Self {
            automation_fn: Box::new(automation_fn),
        }
    }
}

impl<S> AutomatedValue for Automation<S> {
    type Storage = S;
    type Value = f64;

    fn use_context(&mut self, context: &AutomationContext<Self::Storage>) -> f64 {
        (self.automation_fn)(context)
    }
}

#[derive(Clone, Serialize)]
#[serde(untagged)]
pub enum LfSource<C> {
    Value(f64),
    Unit(LfSourceUnit),
    Expr(Box<LfSourceExpr<C>>),
}

impl<'de, C: Deserialize<'de>> Deserialize<'de> for LfSource<C> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(LfSourceVisitor {
            phantom: Default::default(),
        })
    }
}

// Visitor compensating for poor error messages when using untagged enums.
struct LfSourceVisitor<C> {
    phantom: PhantomData<C>,
}

impl<'de, C: Deserialize<'de>> Visitor<'de> for LfSourceVisitor<C> {
    type Value = LfSource<C>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(
            formatter,
            "float value, unit expression or nested LF source expression"
        )
    }

    // Handles the case where a number is provided as an input source
    fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(LfSource::Value(v))
    }

    // Handles the case where a unit variant is provided as an input source
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        LfSourceUnit::deserialize(v.into_deserializer()).map(LfSourceUnit::wrap)
    }

    // Handles the case where a struct variant is provided as an input source
    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: de::MapAccess<'de>,
    {
        LfSourceExpr::deserialize(MapAccessDeserializer::new(map)).map(LfSourceExpr::wrap)
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub enum LfSourceUnit {
    WaveformPitch,
    Wavelength,
}

impl LfSourceUnit {
    pub fn wrap<C>(self) -> LfSource<C> {
        LfSource::Unit(self)
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub enum LfSourceExpr<C> {
    Add(LfSource<C>, LfSource<C>),
    Mul(LfSource<C>, LfSource<C>),
    Oscillator {
        kind: OscillatorKind,
        phase: f64,
        frequency: LfSource<C>,
        baseline: LfSource<C>,
        amplitude: LfSource<C>,
    },
    Envelope {
        name: String,
        from: LfSource<C>,
        to: LfSource<C>,
    },
    Time {
        start: LfSource<C>,
        end: LfSource<C>,
        from: LfSource<C>,
        to: LfSource<C>,
    },
    Property {
        kind: Property,
        from: LfSource<C>,
        to: LfSource<C>,
    },
    Control {
        controller: C,
        from: LfSource<C>,
        to: LfSource<C>,
    },
}

impl<C> LfSourceExpr<C> {
    pub fn wrap(self) -> LfSource<C> {
        LfSource::Expr(Box::new(self))
    }
}

impl<C: Controller> Spec for &LfSource<C> {
    type Created = Automation<C::Storage>;

    fn use_creator(self, creator: &Creator) -> Self::Created {
        match self {
            &LfSource::Value(constant) => Automation::new(move |_| constant),
            LfSource::Unit(unit) => match unit {
                LfSourceUnit::WaveformPitch => Automation::new(move |context| {
                    (context.properties.pitch * context.pitch_bend).as_hz()
                }),
                LfSourceUnit::Wavelength => Automation::new(move |context| {
                    (context.properties.pitch * context.pitch_bend)
                        .as_hz()
                        .recip()
                }),
            },
            LfSource::Expr(expr) => match &**expr {
                LfSourceExpr::Add(a, b) => {
                    let (mut a, mut b) = creator.create((a, b));
                    Automation::new(move |context| context.read(&mut a) + context.read(&mut b))
                }
                LfSourceExpr::Mul(a, b) => {
                    let (mut a, mut b) = creator.create((a, b));
                    Automation::new(move |context| context.read(&mut a) * context.read(&mut b))
                }
                LfSourceExpr::Oscillator {
                    kind,
                    phase,
                    frequency,
                    baseline,
                    amplitude,
                } => match kind {
                    OscillatorKind::Sin => create_oscillator_automation(
                        creator,
                        *phase,
                        frequency,
                        baseline,
                        amplitude,
                        functions::sin,
                    ),
                    OscillatorKind::Sin3 => create_oscillator_automation(
                        creator,
                        *phase,
                        frequency,
                        baseline,
                        amplitude,
                        functions::sin3,
                    ),
                    OscillatorKind::Triangle => create_oscillator_automation(
                        creator,
                        *phase,
                        frequency,
                        baseline,
                        amplitude,
                        functions::triangle,
                    ),
                    OscillatorKind::Square => create_oscillator_automation(
                        creator,
                        *phase,
                        frequency,
                        baseline,
                        amplitude,
                        functions::square,
                    ),
                    OscillatorKind::Sawtooth => create_oscillator_automation(
                        creator,
                        *phase,
                        frequency,
                        baseline,
                        amplitude,
                        functions::sawtooth,
                    ),
                },
                LfSourceExpr::Envelope { name, from, to } => {
                    let envelope = creator.create_envelope(name).unwrap();
                    let mut from_to = creator.create((from, to));

                    Automation::new(move |context| {
                        let (from, to) = context.read(&mut from_to);

                        let envelope_value = envelope.get_value(
                            context.properties.secs_since_pressed,
                            context.properties.secs_since_released,
                        );

                        from + envelope_value * (to - from)
                    })
                }
                LfSourceExpr::Time {
                    start,
                    end,
                    from,
                    to,
                } => {
                    let mut start_end = creator.create((start, end));
                    let mut from_to = creator.create((from, to));

                    Automation::new(move |context| {
                        let (start, end) = context.read(&mut start_end);
                        let (from, to) = context.read(&mut from_to);

                        let curr_time = context.properties.secs_since_pressed;
                        if curr_time <= start && curr_time <= end {
                            from
                        } else if curr_time >= start && curr_time >= end {
                            to
                        } else {
                            from + (to - from) * (curr_time - start) / (end - start)
                        }
                    })
                }
                LfSourceExpr::Property { kind, from, to } => match kind {
                    Property::Velocity => {
                        create_scaled_value_automation(creator, from, to, |context| {
                            context.properties.velocity
                        })
                    }
                    Property::KeyPressure => {
                        create_scaled_value_automation(creator, from, to, |context| {
                            context.properties.pressure
                        })
                    }
                },
                LfSourceExpr::Control {
                    controller,
                    from,
                    to,
                } => {
                    let mut controller = controller.clone();
                    create_scaled_value_automation(creator, from, to, move |context| {
                        context.read(&mut controller)
                    })
                }
            },
        }
    }
}

fn create_scaled_value_automation<C: Controller>(
    creator: &Creator,
    from: &LfSource<C>,
    to: &LfSource<C>,
    mut value_fn: impl FnMut(&AutomationContext<C::Storage>) -> f64 + Send + 'static,
) -> Automation<C::Storage> {
    let mut from_to = creator.create((from, to));

    Automation::new(move |context| {
        let (from, to) = context.read(&mut from_to);

        from + value_fn(context) * (to - from)
    })
}

fn create_oscillator_automation<C: Controller>(
    creator: &Creator,
    mut phase: f64,
    frequency: &LfSource<C>,
    baseline: &LfSource<C>,
    amplitude: &LfSource<C>,
    mut oscillator_fn: impl FnMut(f64) -> f64 + Send + 'static,
) -> Automation<C::Storage> {
    let mut frequency_baseline_amplitude = creator.create((frequency, baseline, amplitude));

    Automation::new(move |context| {
        let (frequency, baseline, amplitude) = context.read(&mut frequency_baseline_amplitude);

        let signal = oscillator_fn(phase);
        phase = (phase + frequency * context.render_window_secs).rem_euclid(1.0);
        baseline + signal * amplitude
    })
}

impl<C> Add for LfSource<C> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        LfSourceExpr::Add(self, rhs).wrap()
    }
}

impl<C> Mul for LfSource<C> {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        LfSourceExpr::Mul(self, rhs).wrap()
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub enum Property {
    Velocity,
    KeyPressure,
}

#[cfg(test)]
mod tests {
    use crate::{magnetron::spec::StageSpec, synth::SynthControl};

    #[test]
    fn deserialize_stage_with_missing_lf_source() {
        let yml = r"
Filter:
  kind: LowPass2
  resonance:
    Control:
      controller: Modulation
      from: 0.0
      to:
  quality: 5.0
  in_buffer: 0
  out_buffer: AudioOut
  out_level: 1.0";
        assert_eq!(
            serde_yaml::from_str::<StageSpec<SynthControl>>(yml)
                .err()
                .unwrap()
                .to_string(),
            "Filter: invalid type: unit value, expected float value, unit expression or nested LF source expression at line 3 column 7"
        )
    }

    #[test]
    fn deserialize_stage_with_integer_lf_source() {
        let yml = r"
Filter:
  kind: LowPass2
  resonance:
    Control:
      controller: Modulation
      from: 0.0
      to: 10000
  quality: 5.0
  in_buffer: 0
  out_buffer: AudioOut
  out_level: 1.0";
        assert_eq!(
            serde_yaml::from_str::<StageSpec<SynthControl>>(yml)
                .err()
                .unwrap()
                .to_string(),
            "Filter: invalid type: integer `10000`, expected float value, unit expression or nested LF source expression at line 3 column 7"
        )
    }

    #[test]
    fn deserialize_stage_with_invalid_unit_lf_source() {
        let yml = r"
Filter:
  kind: LowPass2
  resonance:
    Control:
      controller: Modulation
      from: 0.0
      to: InvalidUnit
  quality: 5.0
  in_buffer: 0
  out_buffer: AudioOut
  out_level: 1.0";
        assert_eq!(
            serde_yaml::from_str::<StageSpec<SynthControl>>(yml)
                .err()
                .unwrap()
                .to_string(),
            "Filter: unknown variant `InvalidUnit`, expected `WaveformPitch` or `Wavelength` at line 3 column 7"
        )
    }

    #[test]
    fn deserialize_stage_with_invalid_lf_source_expression() {
        let yml = r"
Filter:
  kind: LowPass2
  resonance:
    Control:
      controller: Modulation
      from: 0.0
      to:
        InvalidExpr:
  quality: 5.0
  in_buffer: 0
  out_buffer: AudioOut
  out_level: 1.0";
        assert_eq!(
            serde_yaml::from_str::<StageSpec<SynthControl>>(yml)
                .err()
                .unwrap()
                .to_string(),
            "Filter: unknown variant `InvalidExpr`, expected one of `Add`, `Mul`, `Oscillator`, `Envelope`, `Time`, `Property`, `Control` at line 3 column 7"
        )
    }
}
