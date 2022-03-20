use std::{
    fmt::Debug,
    hash::Hash,
    io::Write,
    mem,
    sync::{
        mpsc::{self, Sender},
        Arc,
    },
};

use midir::MidiInputConnection;
use tune::{
    midi::{ChannelMessage, ChannelMessageType},
    note::Note,
    pitch::{Pitch, Pitched},
    scala::{KbmRoot, Scl},
    tuner::{
        AotTuner, JitTuner, MidiTunerMessage, MidiTunerMessageHandler, PoolingMode, SetTuningError,
        TunableMidi,
    },
    tuning::{Scale, Tuning},
};
use tune_cli::{
    shared::midi::{self, MidiInArgs, MidiOutArgs, MidiSource, TuningMethod},
    CliResult,
};

use crate::{
    keypress::{IllegalState, KeypressTracker, LiftAction, PlaceAction},
    piano::{Backend, PianoEngine},
};

pub struct MidiOutBackend<I, S> {
    info_sender: Sender<I>,
    device: String,
    tuning_method: TuningMethod,
    curr_program: u8,
    tuner: MidiTuner<S>,
}

pub fn create<I, S: Copy + Eq + Hash>(
    info_sender: Sender<I>,
    target_port: &str,
    midi_out_args: MidiOutArgs,
    tuning_method: TuningMethod,
) -> CliResult<MidiOutBackend<I, S>> {
    let (device, mut midi_out) = midi::connect_to_out_device("microwave", target_port)?;

    let (midi_send, midi_recv) = mpsc::channel::<MidiTunerMessage>();

    crate::task::spawn(async move {
        for message in midi_recv {
            message.send_to(|m| midi_out.send(m).unwrap());
        }
    });

    let target = midi_out_args.get_midi_target(MidiOutHandler { midi_send })?;
    let synth = midi_out_args.create_synth(target, tuning_method);

    Ok(MidiOutBackend {
        info_sender,
        device,
        tuning_method,
        curr_program: 0,
        tuner: MidiTuner::None { synth },
    })
}

enum MidiTuner<S> {
    Destroyed,
    None {
        synth: TunableMidi<MidiOutHandler>,
    },
    Jit {
        jit_tuner: JitTuner<S, TunableMidi<MidiOutHandler>>,
    },
    Aot {
        aot_tuner: AotTuner<i32, TunableMidi<MidiOutHandler>>,
        keypress_tracker: KeypressTracker<S, i32>,
    },
}

struct MidiOutHandler {
    midi_send: Sender<MidiTunerMessage>,
}

impl MidiTunerMessageHandler for MidiOutHandler {
    fn handle(&mut self, message: MidiTunerMessage) {
        self.midi_send.send(message).unwrap();
    }
}

impl<I: From<MidiInfo> + Send, S: Copy + Eq + Hash + Debug + Send> Backend<S>
    for MidiOutBackend<I, S>
{
    fn set_tuning(&mut self, tuning: (&Scl, KbmRoot)) {
        let synth = self.destroy_tuning();

        let lowest_key = tuning
            .find_by_pitch_sorted(Note::from_midi_number(-1).pitch())
            .approx_value;

        let highest_key = tuning
            .find_by_pitch_sorted(Note::from_midi_number(128).pitch())
            .approx_value;

        let mut aot_tuner = AotTuner::start(synth);

        let tuning = tuning.as_sorted_tuning().as_linear_mapping();
        let keys = lowest_key..highest_key;

        self.tuner = match aot_tuner.set_tuning(tuning, keys) {
            Ok(_) => MidiTuner::Aot {
                aot_tuner,
                keypress_tracker: KeypressTracker::new(),
            },
            Err(SetTuningError::TooManyChannelsRequired(required_channels)) => {
                eprintln!(
                    "[WARNING] Cannot apply tuning. The tuning requires {required_channels} channels",
                );
                MidiTuner::None {
                    synth: aot_tuner.stop(),
                }
            }
            Err(SetTuningError::TunableSynthResult(())) => {
                unreachable!();
            }
        };

        self.send_status();
    }

    fn set_no_tuning(&mut self) {
        let synth = self.destroy_tuning();
        let jit_tuner = JitTuner::start(synth, PoolingMode::Stop);
        self.tuner = MidiTuner::Jit { jit_tuner };

        self.send_status();
    }

    fn send_status(&self) {
        let is_tuned = match self.tuner {
            MidiTuner::Destroyed | MidiTuner::None { .. } => false,
            MidiTuner::Jit { .. } | MidiTuner::Aot { .. } => true,
        };

        self.info_sender
            .send(
                MidiInfo {
                    device: self.device.clone(),
                    program_number: self.curr_program,
                    tuning_method: is_tuned.then(|| self.tuning_method),
                }
                .into(),
            )
            .unwrap();
    }

    fn start(&mut self, id: S, degree: i32, pitch: Pitch, velocity: u8) {
        match &mut self.tuner {
            MidiTuner::Destroyed | MidiTuner::None { .. } => {}
            MidiTuner::Jit { jit_tuner } => {
                jit_tuner.note_on(id, pitch, velocity);
            }
            MidiTuner::Aot {
                keypress_tracker,
                aot_tuner,
            } => match keypress_tracker.place_finger_at(id, degree) {
                Ok(PlaceAction::KeyPressed) => {
                    aot_tuner.note_on(degree, velocity);
                }
                Ok(PlaceAction::KeyAlreadyPressed) => {
                    aot_tuner.note_off(degree, velocity);
                    aot_tuner.note_on(degree, velocity);
                }
                Err(id) => {
                    eprintln!(
                        "[WARNING] Key with ID {:?} not lifted before pressed again",
                        id,
                    );
                }
            },
        }
    }

    fn update_pitch(&mut self, id: S, degree: i32, pitch: Pitch, velocity: u8) {
        match &mut self.tuner {
            MidiTuner::Destroyed | MidiTuner::None { .. } => {}
            MidiTuner::Jit { jit_tuner } => {
                jit_tuner.note_pitch(id, pitch);
            }
            MidiTuner::Aot {
                keypress_tracker,
                aot_tuner,
            } => match keypress_tracker.move_finger_to(&id, degree) {
                Ok((LiftAction::KeyReleased(released), _)) => {
                    aot_tuner.note_off(released, velocity);
                    aot_tuner.note_on(degree, velocity);
                }
                Ok((LiftAction::KeyRemainsPressed, PlaceAction::KeyPressed)) => {
                    aot_tuner.note_on(degree, velocity);
                }
                Ok((LiftAction::KeyRemainsPressed, PlaceAction::KeyAlreadyPressed)) => {}
                Err(IllegalState) => {}
            },
        }
    }

    fn update_pressure(&mut self, id: S, pressure: u8) {
        match &mut self.tuner {
            MidiTuner::Destroyed | MidiTuner::None { .. } => {}
            MidiTuner::Jit { jit_tuner } => {
                jit_tuner.note_attr(id, pressure);
            }
            MidiTuner::Aot {
                keypress_tracker,
                aot_tuner,
            } => {
                if let Some(&location) = keypress_tracker.location_of(&id) {
                    aot_tuner.note_attr(location, pressure);
                }
            }
        }
    }

    fn stop(&mut self, id: S, velocity: u8) {
        match &mut self.tuner {
            MidiTuner::Destroyed | MidiTuner::None { .. } => {}
            MidiTuner::Jit { jit_tuner } => {
                jit_tuner.note_off(id, velocity);
            }
            MidiTuner::Aot {
                keypress_tracker,
                aot_tuner,
            } => match keypress_tracker.lift_finger(&id) {
                Ok(LiftAction::KeyReleased(location)) => aot_tuner.note_off(location, velocity),
                Ok(LiftAction::KeyRemainsPressed) => {}
                Err(IllegalState) => {}
            },
        }
    }

    fn program_change(&mut self, mut update_fn: Box<dyn FnMut(usize) -> usize + Send>) {
        self.curr_program =
            u8::try_from(update_fn(usize::from(self.curr_program) + 128) % 128).unwrap();

        self.send_monophonic_message(ChannelMessageType::ProgramChange {
            program: self.curr_program,
        });

        self.send_status();
    }

    fn control_change(&mut self, controller: u8, value: u8) {
        self.send_monophonic_message(ChannelMessageType::ControlChange { controller, value });
    }

    fn channel_pressure(&mut self, pressure: u8) {
        self.send_monophonic_message(ChannelMessageType::ChannelPressure { pressure });
    }

    fn pitch_bend(&mut self, value: i16) {
        self.send_monophonic_message(ChannelMessageType::PitchBendChange { value });
    }

    fn toggle_envelope_type(&mut self) {}

    fn has_legato(&self) -> bool {
        true
    }
}

impl<I, S: Copy + Eq + Hash> MidiOutBackend<I, S> {
    fn destroy_tuning(&mut self) -> TunableMidi<MidiOutHandler> {
        let mut tuner = MidiTuner::Destroyed;
        mem::swap(&mut tuner, &mut self.tuner);

        match tuner {
            MidiTuner::None { synth } => synth,
            MidiTuner::Jit { jit_tuner } => jit_tuner.stop(),
            MidiTuner::Aot {
                mut aot_tuner,
                keypress_tracker,
            } => {
                for pressed_key in keypress_tracker.pressed_locations() {
                    aot_tuner.note_off(pressed_key, 0);
                }
                aot_tuner.stop()
            }
            MidiTuner::Destroyed => unreachable!("Tuning already destroyed"),
        }
    }

    fn send_monophonic_message(&mut self, message_type: ChannelMessageType) {
        match &mut self.tuner {
            MidiTuner::None { .. } => {}
            MidiTuner::Jit { jit_tuner } => {
                jit_tuner.global_attr(message_type);
            }
            MidiTuner::Aot { aot_tuner, .. } => {
                aot_tuner.global_attr(message_type);
            }
            MidiTuner::Destroyed => {}
        }
    }
}

pub struct MidiInfo {
    pub device: String,
    pub tuning_method: Option<TuningMethod>,
    pub program_number: u8,
}

pub fn connect_to_midi_device(
    mut engine: Arc<PianoEngine>,
    target_port: &str,
    midi_in_args: MidiInArgs,
    midi_logging: bool,
) -> CliResult<(String, MidiInputConnection<()>)> {
    let midi_source = midi_in_args.get_midi_source()?;

    Ok(midi::connect_to_in_device(
        "microwave",
        target_port,
        move |message| process_midi_event(message, &mut engine, &midi_source, midi_logging),
    )?)
}

fn process_midi_event(
    message: &[u8],
    engine: &mut Arc<PianoEngine>,
    midi_source: &MidiSource,
    midi_logging: bool,
) {
    let stderr = std::io::stderr();
    let mut stderr = stderr.lock();
    if let Some(channel_message) = ChannelMessage::from_raw_message(message) {
        if midi_logging {
            writeln!(stderr, "[DEBUG] MIDI message received:").unwrap();
            writeln!(stderr, "{:#?}", channel_message).unwrap();
            writeln!(stderr,).unwrap();
        }
        if midi_source.channels.contains(&channel_message.channel()) {
            engine.handle_midi_event(
                channel_message.message_type(),
                midi_source.get_offset(channel_message.channel()),
            );
        }
    } else {
        writeln!(stderr, "[WARNING] Unsupported MIDI message received:").unwrap();
        for i in message {
            writeln!(stderr, "{:08b}", i).unwrap();
        }
        writeln!(stderr).unwrap();
    }
}
