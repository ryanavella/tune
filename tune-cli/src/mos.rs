use std::convert::TryInto;
use std::{cmp::Ordering, io};
use tune::{generators::Meantone, math, ratio::Ratio};

struct Heptatonic {
    meantone: Meantone,
}

impl Heptatonic {
    pub fn from_edo(num_divisions: i16) -> Heptatonic {
        let meantone = Meantone::for_edo(num_divisions as u16);
        Self { meantone }
    }

    // Locrian    (sLLsLLL)
    // Phrygian   (sLLLsLL)
    // Aeolian    (LsLLsLL)
    // Dorian     (LsLLLsL)
    // Mixolydian (LLsLLsL)
    // Ionian     (LLsLLLs)
    // Lydian     (LLLsLLs)

    // MelodicMinor    (LsLLLLs)
    // DorianFlat2     (sLLLLsL)
    // LydianAugmented (LLLLsLs)
    // LydianDominant  (LLLsLsL)
    // MelodicMajor    (LLsLsLL)
    // HalfDiminished  (LsLsLLLL)
    // Altered         (sLsLLLL)

    fn print_info(&self, mut target: impl io::Write) -> io::Result<()> {
        writeln!(
            target,
            "---- Properties of {}-EDO ----",
            self.meantone.num_divisions()
        )?;
        writeln!(target, "Number of cycles: {}", self.meantone.num_cycles())?;
        writeln!(
            target,
            "Fifth: {} EDO steps = {:#} = Pythagorean {:#}",
            self.meantone.num_steps_of_fifth(),
            self.meantone.size_of_fifth(),
            Ratio::from_cents(
                self.meantone.size_of_fifth().as_cents() - Ratio::from_float(1.5).as_cents()
            ),
            // TODO: Ratio::between_ratios
        )?;
        writeln!(
            target,
            "1 large step = {} EDO steps",
            self.meantone.large_step()
        )?;
        writeln!(
            target,
            "1 small step = {} EDO steps",
            self.meantone.small_step()
        )?;
        writeln!(target, "1 sharp = {} EDO steps", self.meantone.sharpness())?;
        writeln!(
            target,
            "Dorian scale: {} {} {} {} {} {} {} {}",
            0,
            self.meantone.large_step(),
            self.meantone.large_step() + self.meantone.small_step(),
            2 * self.meantone.large_step() + self.meantone.small_step(),
            3 * self.meantone.large_step() + self.meantone.small_step(),
            4 * self.meantone.large_step() + self.meantone.small_step(),
            4 * self.meantone.large_step() + 2 * self.meantone.small_step(),
            5 * self.meantone.large_step() + 2 * self.meantone.small_step()
        )?;
        writeln!(target, "Scale steps")?;
        for (index, note) in self.find_names().iter().enumerate() {
            writeln!(target, "{:>3}. {}", index, note.sharps_and_flats_name())?;
        }
        // TODO: More Just intervals?

        writeln!(target)?;
        Ok(())
    }

    pub fn find_names(&self) -> Vec<GeneralizedNote> {
        let mut note_names = vec![String::new(); self.meantone.num_divisions().try_into().unwrap()];
        for (num_fifths, letter) in &[
            (1, "A"),
            (3, "B"),
            (-2, "C"),
            (0, "D"),
            (2, "E"),
            (-3, "F"),
            (-1, "G"),
        ] {
            let index: usize = (num_fifths * i32::from(self.meantone.num_steps_of_fifth()))
                .rem_euclid(i32::from(self.meantone.num_divisions()))
                .try_into()
                .unwrap();
            note_names[index].push_str(letter);
        }

        let mut notes_with_ups = Vec::new();
        let mut lower_note_name = &note_names[0];
        let mut up_count = 0;
        for precise_note_name in &note_names {
            if !precise_note_name.is_empty() {
                lower_note_name = precise_note_name;
                up_count = 0;
            } else {
                up_count += 1;
            }
            notes_with_ups.push((lower_note_name, up_count));
        }

        let mut notes_with_ups_and_downs = Vec::new();
        let mut upper_note_name = &note_names[0]; // necessary?
        let mut down_count = 0;
        for &(lower_note_name, up_count) in notes_with_ups.iter().rev() {
            if up_count == 0 {
                upper_note_name = lower_note_name;
                down_count = 0;
            } else {
                down_count += 1;
            }
            notes_with_ups_and_downs.push((lower_note_name, up_count, upper_note_name, down_count));
        }

        notes_with_ups_and_downs
            .into_iter()
            .rev()
            .map(|(lower_name, up_count, upper_name, down_count)| {
                if up_count == 0 {
                    assert!(upper_name == lower_name);
                    assert!(down_count == 0);
                    GeneralizedNote::Precise {
                        name: lower_name.clone(),
                    }
                } else {
                    GeneralizedNote::InBetween {
                        lower_name: lower_name.clone(),
                        up_count,
                        upper_name: upper_name.clone(),
                        down_count,
                        sharpness: self.meantone.sharpness(),
                    }
                }
            })
            .collect()
    }
}

fn gcd_i16(numer: i16, denom: i16) -> i16 {
    math::gcd_u16(numer.abs() as u16, denom.abs() as u16) as i16
}

#[derive(Clone, Debug)]
pub enum GeneralizedNote {
    Precise {
        name: String,
    },
    InBetween {
        lower_name: String,
        up_count: u16,
        upper_name: String,
        down_count: u16,
        sharpness: i16,
    },
}

impl GeneralizedNote {
    pub fn ups_and_downs_name(&self) -> String {
        let mut with_sharpness_zero = self.clone();
        match with_sharpness_zero {
            GeneralizedNote::Precise { name: _ } => {}
            GeneralizedNote::InBetween {
                lower_name: _,
                up_count: _,
                upper_name: _,
                down_count: _,
                ref mut sharpness,
            } => {
                *sharpness = 0;
            }
        }
        with_sharpness_zero.sharps_and_flats_name()
    }

    pub fn sharps_and_flats_name(&self) -> String {
        match self {
            GeneralizedNote::Precise { name } => name.clone(),
            GeneralizedNote::InBetween {
                lower_name,
                up_count,
                upper_name,
                down_count,
                sharpness,
            } => match up_count.cmp(down_count) {
                Ordering::Less => format!(
                    "{}{}",
                    lower_name,
                    render_sharp_or_flat_ratio(*up_count, *sharpness, true)
                ),
                Ordering::Greater => format!(
                    "{}{}",
                    upper_name,
                    render_sharp_or_flat_ratio(*down_count, -*sharpness, false)
                ),
                Ordering::Equal => format!(
                    "{}{} / {}{}",
                    lower_name,
                    render_sharp_or_flat_ratio(*up_count, *sharpness, true),
                    upper_name,
                    render_sharp_or_flat_ratio(*down_count, -*sharpness, false)
                ),
            },
        }
    }
}

fn render_sharp_or_flat_ratio(up_or_down_count: u16, sharpness: i16, is_up: bool) -> String {
    if sharpness < 0 {
        render_accidental("b", up_or_down_count, -sharpness as u16)
    } else if sharpness > 0 {
        render_accidental("#", up_or_down_count, sharpness as u16)
    } else {
        let accidental = if is_up { "^" } else { "v" };
        render_accidental(accidental, up_or_down_count, 1)
    }
}

fn render_accidental(accidental: &str, mut numer: u16, denom: u16) -> String {
    let mut rendered = String::new();
    while numer >= denom {
        rendered.push_str(accidental);
        numer -= denom;
    }
    let (numer, denom) = math::simplify_u16(numer, denom);
    if denom == 1 {
        rendered
    } else {
        format!("{}[{}/{}]{}", rendered, numer, denom, accidental)
    }
}

struct Keyboard {
    large_step: i16,
    small_step: i16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_ups_and_downs() {
        let small_edo = Heptatonic::from_edo(6).find_names();
        assert_eq!(small_edo[0].ups_and_downs_name(), "BDF");
        assert_eq!(small_edo[1].ups_and_downs_name(), "BDF^ / EGv");
        assert_eq!(small_edo[2].ups_and_downs_name(), "EG");
        assert_eq!(small_edo[3].ups_and_downs_name(), "EG^ / ACv");
        assert_eq!(small_edo[4].ups_and_downs_name(), "AC");
        assert_eq!(small_edo[5].ups_and_downs_name(), "AC^ / BDFv");

        let neg_sharp_edo = Heptatonic::from_edo(16).find_names();
        assert_eq!(neg_sharp_edo[0].ups_and_downs_name(), "D");
        assert_eq!(neg_sharp_edo[1].ups_and_downs_name(), "D^ / Ev");
        assert_eq!(neg_sharp_edo[2].ups_and_downs_name(), "E");
        assert_eq!(neg_sharp_edo[3].ups_and_downs_name(), "E^");
        assert_eq!(neg_sharp_edo[4].ups_and_downs_name(), "Fv");
        assert_eq!(neg_sharp_edo[5].ups_and_downs_name(), "F");
    }

    #[test]
    fn render_sharps_and_flats() {
        let small_edo = Heptatonic::from_edo(6).find_names();
        assert_eq!(small_edo[0].sharps_and_flats_name(), "BDF");
        assert_eq!(small_edo[1].sharps_and_flats_name(), "BDF[1/4]# / EG[1/4]b");
        assert_eq!(small_edo[2].sharps_and_flats_name(), "EG");
        assert_eq!(small_edo[3].sharps_and_flats_name(), "EG[1/4]# / AC[1/4]b");
        assert_eq!(small_edo[4].sharps_and_flats_name(), "AC");
        assert_eq!(small_edo[5].sharps_and_flats_name(), "AC[1/4]# / BDF[1/4]b");

        let normal_edo = Heptatonic::from_edo(26).find_names();
        assert_eq!(normal_edo[0].sharps_and_flats_name(), "D");
        assert_eq!(normal_edo[1].sharps_and_flats_name(), "D#");
        assert_eq!(normal_edo[2].sharps_and_flats_name(), "D## / Ebb");
        assert_eq!(normal_edo[3].sharps_and_flats_name(), "Eb");
        assert_eq!(normal_edo[4].sharps_and_flats_name(), "E");
        assert_eq!(normal_edo[5].sharps_and_flats_name(), "E#");
        assert_eq!(normal_edo[6].sharps_and_flats_name(), "Fb");
        assert_eq!(normal_edo[7].sharps_and_flats_name(), "F");

        let neg_sharp_edo = Heptatonic::from_edo(16).find_names();
        assert_eq!(neg_sharp_edo[0].sharps_and_flats_name(), "D");
        assert_eq!(neg_sharp_edo[1].sharps_and_flats_name(), "Db / E#");
        assert_eq!(neg_sharp_edo[2].sharps_and_flats_name(), "E");
        assert_eq!(neg_sharp_edo[3].sharps_and_flats_name(), "Eb");
        assert_eq!(neg_sharp_edo[4].sharps_and_flats_name(), "F#");
        assert_eq!(neg_sharp_edo[5].sharps_and_flats_name(), "F");

        let zero_sharp_edo = Heptatonic::from_edo(28).find_names();
        assert_eq!(zero_sharp_edo[0].sharps_and_flats_name(), "D");
        assert_eq!(zero_sharp_edo[1].sharps_and_flats_name(), "D^");
        assert_eq!(zero_sharp_edo[2].sharps_and_flats_name(), "D^^ / Evv");
        assert_eq!(zero_sharp_edo[3].sharps_and_flats_name(), "Ev");
        assert_eq!(zero_sharp_edo[4].sharps_and_flats_name(), "E");
        assert_eq!(zero_sharp_edo[5].sharps_and_flats_name(), "E^");
        assert_eq!(zero_sharp_edo[6].sharps_and_flats_name(), "E^^ / Fvv");
        assert_eq!(zero_sharp_edo[7].sharps_and_flats_name(), "Fv");
        assert_eq!(zero_sharp_edo[8].sharps_and_flats_name(), "F");

        let fractional_accidental = &Heptatonic::from_edo(38).find_names()[3];
        assert_eq!(
            fractional_accidental.sharps_and_flats_name(),
            "D#[1/2]# / Eb[1/2]b",
        );
    }

    #[test]
    fn edo_summary() {
        let mut output = io::stdout();
        for num_divisions in 1..100 {
            let tet = Heptatonic::from_edo(num_divisions);
            tet.print_info(&mut output).unwrap();
        }
    }
}
