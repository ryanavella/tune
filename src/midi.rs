// https://www.midi.org/specifications-old/item/table-1-summary-of-midi-message

pub const NOTE_OFF: u8 = 0b1000;
pub const NOTE_ON: u8 = 0b1001;
pub const POLYPHONIC_KEY_PRESSURE: u8 = 0b1010;
pub const CONTROL_CHANGE: u8 = 0b1011;
pub const PROGRAM_CHANGE: u8 = 0b1100;
pub const CHANNEL_PRESSURE: u8 = 0b1101;
pub const PITCH_BEND_CHANGE: u8 = 0b1110;

#[derive(Clone, Debug)]
pub struct ChannelMessage {
    pub channel: u8,
    pub message_type: ChannelMessageType,
}

impl ChannelMessage {
    pub fn from_raw_message(message: &[u8]) -> Option<ChannelMessage> {
        let status_byte = *message.get(0)?;
        let channel = status_byte & 0b0000_1111;
        let action = status_byte >> 4;
        let message_type = match action {
            NOTE_OFF => ChannelMessageType::NoteOff {
                key: *message.get(1)?,
                velocity: *message.get(2)?,
            },
            NOTE_ON => ChannelMessageType::NoteOn {
                key: *message.get(1)?,
                velocity: *message.get(2)?,
            },
            POLYPHONIC_KEY_PRESSURE => ChannelMessageType::PolyphonicKeyPressure {
                key: *message.get(1)?,
                pressure: *message.get(2)?,
            },
            CONTROL_CHANGE => ChannelMessageType::ControlChange {
                controller: *message.get(1)?,
                value: *message.get(2)?,
            },
            PROGRAM_CHANGE => ChannelMessageType::ProgramChange {
                program: *message.get(1)?,
            },
            CHANNEL_PRESSURE => ChannelMessageType::ChannelPressure {
                pressure: *message.get(1)?,
            },
            PITCH_BEND_CHANGE => ChannelMessageType::PitchBendChange {
                value: u32::from(*message.get(1)?) + u32::from(*message.get(2)?) * 128,
            },
            _ => return None,
        };
        Some(ChannelMessage {
            channel,
            message_type,
        })
    }
}

#[derive(Copy, Clone, Debug)]
pub enum ChannelMessageType {
    NoteOff { key: u8, velocity: u8 },
    NoteOn { key: u8, velocity: u8 },
    PolyphonicKeyPressure { key: u8, pressure: u8 },
    ControlChange { controller: u8, value: u8 },
    ProgramChange { program: u8 },
    ChannelPressure { pressure: u8 },
    PitchBendChange { value: u32 },
}

// TODO: Tuning-specific code in tuning module

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_note_off() {
        let message = ChannelMessage::from_raw_message(&[0b1000_0111, 88, 99]).unwrap();
        assert!(matches!(
            message,
            ChannelMessage {
                channel: 7,
                message_type: ChannelMessageType::NoteOff {
                    key: 88,
                    velocity: 99
                }
            }
        ));
    }

    #[test]
    fn parse_note_on() {
        let message = ChannelMessage::from_raw_message(&[0b1001_1000, 77, 88]).unwrap();
        assert!(matches!(
            message,
            ChannelMessage {
                channel: 8,
                message_type: ChannelMessageType::NoteOn {
                    key: 77,
                    velocity: 88
                }
            }
        ));
    }

    #[test]
    fn parse_polyphonic_key_pressure() {
        let message = ChannelMessage::from_raw_message(&[0b1010_1001, 66, 77]).unwrap();
        assert!(matches!(
            message,
            ChannelMessage {
                channel: 9,
                message_type: ChannelMessageType::PolyphonicKeyPressure {
                    key: 66,
                    pressure: 77
                }
            }
        ));
    }

    #[test]
    fn parse_control_change() {
        let message = ChannelMessage::from_raw_message(&[0b1011_1010, 55, 66]).unwrap();
        assert!(matches!(
            message,
            ChannelMessage {
                channel: 10,
                message_type: ChannelMessageType::ControlChange {
                    controller: 55,
                    value: 66
                }
            }
        ));
    }

    #[test]
    fn parse_program_change() {
        let message = ChannelMessage::from_raw_message(&[0b1100_1011, 44]).unwrap();
        assert!(matches!(
            message,
            ChannelMessage {
                channel: 11,
                message_type: ChannelMessageType::ProgramChange { program: 44 }
            }
        ));
    }

    #[test]
    fn parse_channel_pressure() {
        let message = ChannelMessage::from_raw_message(&[0b1101_1100, 33]).unwrap();
        assert!(matches!(
            message,
            ChannelMessage {
                channel: 12,
                message_type: ChannelMessageType::ChannelPressure { pressure: 33 }
            }
        ));
    }

    #[test]
    fn parse_pitch_bend_change() {
        let message = ChannelMessage::from_raw_message(&[0b1110_1101, 22, 33]).unwrap();
        assert!(matches!(
            message,
            ChannelMessage {
                channel: 13,
                message_type: ChannelMessageType::PitchBendChange { value: 4246 }
            }
        ));
    }
}
