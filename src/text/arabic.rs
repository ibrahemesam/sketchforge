//! Arabic shaping and script detection.
//!
//! Arabic letters change shape depending on their neighbours. Sketch renders
//! glyph outlines directly, so it maps each letter to the matching Arabic
//! Presentation Forms code point before laying the run out right to left.
//! This covers joining forms and the four lam-alef ligatures, which is enough
//! for ordinary Arabic, Persian and Urdu words without pulling in a full
//! HarfBuzz port.

/// A shaped cell: the code point to draw and how many source chars it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shaped {
    pub ch: char,
    pub len: usize,
}

/// Whether a code point belongs to an Arabic script block.
pub fn is_arabic(c: char) -> bool {
    matches!(
        c as u32,
        0x0600..=0x06ff
            | 0x0750..=0x077f
            | 0x0870..=0x089f
            | 0x08a0..=0x08ff
            | 0xfb50..=0xfdff
            | 0xfe70..=0xfeff
    )
}

/// Whether laying out `text` needs the Arabic shaper (and right-to-left order).
pub fn has_arabic(text: &str) -> bool {
    text.chars().any(is_arabic)
}

/// Shape every character. `None` means the character was folded into the
/// preceding lam-alef ligature.
pub fn shape(chars: &[char]) -> Vec<Option<Shaped>> {
    let mut out: Vec<Option<Shaped>> = vec![None; chars.len()];
    let mut consumed = vec![false; chars.len()];
    for (i, &c) in chars.iter().enumerate() {
        if consumed[i] {
            continue;
        }
        if !is_arabic(c) {
            out[i] = Some(Shaped { ch: c, len: 1 });
            continue;
        }
        if joining(c) == Joining::T {
            out[i] = Some(Shaped { ch: c, len: 1 });
            continue;
        }
        if c == '\u{0644}' {
            // Lam followed by an alef becomes one of the four FEF5..FEFC ligatures.
            if let Some(j) = next_letter(chars, i)
                && let Some(lig) = alef_ligature(chars[j])
            {
                let final_form = prev_letter(chars, i).is_some();
                out[i] = Some(Shaped {
                    ch: ligature(lig, final_form),
                    len: j - i + 1,
                });
                for slot in consumed.iter_mut().take(j + 1).skip(i + 1) {
                    *slot = true;
                }
                continue;
            }
        }
        let prev = prev_letter(chars, i);
        let next = next_letter(chars, i);
        let joins_prev = prev.is_some_and(|p| connects_left(chars[p]) && connects_right(c));
        let joins_next = next.is_some_and(|n| connects_left(c) && connects_right(chars[n]));
        out[i] = Some(Shaped {
            ch: presentation(c, joins_prev, joins_next).unwrap_or(c),
            len: 1,
        });
    }
    out
}

fn presentation(c: char, joins_prev: bool, joins_next: bool) -> Option<char> {
    let forms = match c {
        '\u{0621}' => ['\u{fe80}', '\0', '\0', '\0'],
        '\u{0622}' => ['\u{fe81}', '\u{fe82}', '\0', '\0'],
        '\u{0623}' => ['\u{fe83}', '\u{fe84}', '\0', '\0'],
        '\u{0624}' => ['\u{fe85}', '\u{fe86}', '\0', '\0'],
        '\u{0625}' => ['\u{fe87}', '\u{fe88}', '\0', '\0'],
        '\u{0626}' => ['\u{fe89}', '\u{fe8a}', '\u{fe8b}', '\u{fe8c}'],
        '\u{0627}' => ['\u{fe8d}', '\u{fe8e}', '\0', '\0'],
        '\u{0628}' => ['\u{fe8f}', '\u{fe90}', '\u{fe91}', '\u{fe92}'],
        '\u{0629}' => ['\u{fe93}', '\u{fe94}', '\0', '\0'],
        '\u{062a}' => ['\u{fe95}', '\u{fe96}', '\u{fe97}', '\u{fe98}'],
        '\u{062b}' => ['\u{fe99}', '\u{fe9a}', '\u{fe9b}', '\u{fe9c}'],
        '\u{062c}' => ['\u{fe9d}', '\u{fe9e}', '\u{fe9f}', '\u{fea0}'],
        '\u{062d}' => ['\u{fea1}', '\u{fea2}', '\u{fea3}', '\u{fea4}'],
        '\u{062e}' => ['\u{fea5}', '\u{fea6}', '\u{fea7}', '\u{fea8}'],
        '\u{062f}' => ['\u{fea9}', '\u{feaa}', '\0', '\0'],
        '\u{0630}' => ['\u{feab}', '\u{feac}', '\0', '\0'],
        '\u{0631}' => ['\u{fead}', '\u{feae}', '\0', '\0'],
        '\u{0632}' => ['\u{feaf}', '\u{feb0}', '\0', '\0'],
        '\u{0633}' => ['\u{feb1}', '\u{feb2}', '\u{feb3}', '\u{feb4}'],
        '\u{0634}' => ['\u{feb5}', '\u{feb6}', '\u{feb7}', '\u{feb8}'],
        '\u{0635}' => ['\u{feb9}', '\u{feba}', '\u{febb}', '\u{febc}'],
        '\u{0636}' => ['\u{febd}', '\u{febe}', '\u{febf}', '\u{fec0}'],
        '\u{0637}' => ['\u{fec1}', '\u{fec2}', '\u{fec3}', '\u{fec4}'],
        '\u{0638}' => ['\u{fec5}', '\u{fec6}', '\u{fec7}', '\u{fec8}'],
        '\u{0639}' => ['\u{fec9}', '\u{feca}', '\u{fecb}', '\u{fecc}'],
        '\u{063a}' => ['\u{fecd}', '\u{fece}', '\u{fecf}', '\u{fed0}'],
        '\u{0641}' => ['\u{fed1}', '\u{fed2}', '\u{fed3}', '\u{fed4}'],
        '\u{0642}' => ['\u{fed5}', '\u{fed6}', '\u{fed7}', '\u{fed8}'],
        '\u{0643}' => ['\u{fed9}', '\u{feda}', '\u{fedb}', '\u{fedc}'],
        '\u{0644}' => ['\u{fedd}', '\u{fede}', '\u{fedf}', '\u{fee0}'],
        '\u{0645}' => ['\u{fee1}', '\u{fee2}', '\u{fee3}', '\u{fee4}'],
        '\u{0646}' => ['\u{fee5}', '\u{fee6}', '\u{fee7}', '\u{fee8}'],
        '\u{0647}' => ['\u{fee9}', '\u{feea}', '\u{feeb}', '\u{feec}'],
        '\u{0648}' => ['\u{feed}', '\u{feee}', '\0', '\0'],
        '\u{0649}' => ['\u{feef}', '\u{fef0}', '\0', '\0'],
        '\u{064a}' => ['\u{fef1}', '\u{fef2}', '\u{fef3}', '\u{fef4}'],
        '\u{066e}' => return None,
        '\u{066f}' => return None,
        '\u{0671}' => ['\u{fb50}', '\u{fb51}', '\0', '\0'],
        '\u{0679}' => ['\u{fb66}', '\u{fb67}', '\u{fb68}', '\u{fb69}'],
        '\u{067a}' => ['\u{fb5e}', '\u{fb5f}', '\u{fb60}', '\u{fb61}'],
        '\u{067b}' => ['\u{fb52}', '\u{fb53}', '\u{fb54}', '\u{fb55}'],
        '\u{067e}' => ['\u{fb56}', '\u{fb57}', '\u{fb58}', '\u{fb59}'],
        '\u{067f}' => ['\u{fb62}', '\u{fb63}', '\u{fb64}', '\u{fb65}'],
        '\u{0680}' => ['\u{fb5a}', '\u{fb5b}', '\u{fb5c}', '\u{fb5d}'],
        '\u{0683}' => ['\u{fb76}', '\u{fb77}', '\u{fb78}', '\u{fb79}'],
        '\u{0684}' => ['\u{fb72}', '\u{fb73}', '\u{fb74}', '\u{fb75}'],
        '\u{0686}' => ['\u{fb7a}', '\u{fb7b}', '\u{fb7c}', '\u{fb7d}'],
        '\u{0687}' => ['\u{fb7e}', '\u{fb7f}', '\u{fb80}', '\u{fb81}'],
        '\u{0688}' => ['\u{fb88}', '\u{fb89}', '\0', '\0'],
        '\u{0691}' => ['\u{fb8c}', '\u{fb8d}', '\0', '\0'],
        '\u{0698}' => ['\u{fb8a}', '\u{fb8b}', '\0', '\0'],
        '\u{06a9}' => ['\u{fb8e}', '\u{fb8f}', '\u{fb90}', '\u{fb91}'],
        '\u{06af}' => ['\u{fb92}', '\u{fb93}', '\u{fb94}', '\u{fb95}'],
        '\u{06ba}' => ['\u{fb9e}', '\u{fb9f}', '\0', '\0'],
        '\u{06bb}' => ['\u{fba0}', '\u{fba1}', '\u{fba2}', '\u{fba3}'],
        '\u{06c0}' => ['\u{fba4}', '\u{fba5}', '\0', '\0'],
        '\u{06c1}' => ['\u{fba6}', '\u{fba7}', '\u{fba8}', '\u{fba9}'],
        '\u{06cc}' => ['\u{fbfc}', '\u{fbfd}', '\u{fbfe}', '\u{fbff}'],
        '\u{06d2}' => ['\u{fbae}', '\u{fbaf}', '\0', '\0'],
        '\u{06d5}' => ['\u{fee9}', '\u{feea}', '\0', '\0'],
        _ => return None,
    };
    let pick = match (joins_prev, joins_next) {
        (true, true) => forms[3],
        (true, false) => forms[1],
        (false, true) => forms[2],
        (false, false) => forms[0],
    };
    if pick == '\0' { None } else { Some(pick) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Joining {
    /// Non-joining.
    U,
    /// Joins only to the letter on its right (previous in logical order).
    R,
    /// Joins on both sides.
    D,
    /// Combining mark; ignored when looking for joining neighbours.
    T,
    /// Join-causing (tatweel).
    C,
}

fn joining(c: char) -> Joining {
    match c as u32 {
        0x0621 => Joining::U,
        0x0622..=0x0625 | 0x0627 | 0x0629 | 0x062f..=0x0632 | 0x0648 | 0x0649 => Joining::R,
        0x0626 | 0x0628 | 0x062a..=0x062e | 0x0633..=0x063f | 0x0641..=0x0647 | 0x064a => {
            Joining::D
        }
        0x0640 => Joining::C,
        0x066e | 0x066f => Joining::D,
        0x0671..=0x0678 | 0x0688..=0x0691 | 0x0698 | 0x06ba | 0x06c0 | 0x06c3 | 0x06d2 | 0x06d5 => {
            Joining::R
        }
        0x0679..=0x0687 | 0x06a9..=0x06af | 0x06bb..=0x06be | 0x06c1 | 0x06cc => Joining::D,
        0x0610..=0x061a
        | 0x064b..=0x065f
        | 0x0670
        | 0x06d6..=0x06dc
        | 0x06df..=0x06e8
        | 0x06ea..=0x06ed
        | 0x08d3..=0x08ff => Joining::T,
        _ => Joining::U,
    }
}

fn connects_right(c: char) -> bool {
    matches!(joining(c), Joining::R | Joining::D | Joining::C)
}

fn connects_left(c: char) -> bool {
    matches!(joining(c), Joining::D | Joining::C)
}

fn prev_letter(chars: &[char], i: usize) -> Option<usize> {
    for j in (0..i).rev() {
        let c = chars[j];
        if !is_arabic(c) || c == '\n' {
            return None;
        }
        match joining(c) {
            Joining::T => continue,
            Joining::U => return None,
            _ => return Some(j),
        }
    }
    None
}

fn next_letter(chars: &[char], i: usize) -> Option<usize> {
    for (j, &c) in chars.iter().enumerate().skip(i + 1) {
        if !is_arabic(c) || c == '\n' {
            return None;
        }
        match joining(c) {
            Joining::T => continue,
            Joining::U => return None,
            _ => return Some(j),
        }
    }
    None
}

fn alef_ligature(c: char) -> Option<char> {
    match c {
        '\u{0622}' | '\u{0623}' | '\u{0625}' | '\u{0627}' => Some(c),
        _ => None,
    }
}

fn ligature(alef: char, final_form: bool) -> char {
    match (alef, final_form) {
        ('\u{0622}', false) => '\u{fef5}',
        ('\u{0622}', true) => '\u{fef6}',
        ('\u{0623}', false) => '\u{fef7}',
        ('\u{0623}', true) => '\u{fef8}',
        ('\u{0625}', false) => '\u{fef9}',
        ('\u{0625}', true) => '\u{fefa}',
        (_, false) => '\u{fefb}',
        (_, true) => '\u{fefc}',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forms(text: &str) -> Vec<Option<Shaped>> {
        let chars: Vec<char> = text.chars().collect();
        shape(&chars)
    }

    #[test]
    fn isolated_and_joined_forms() {
        let s = forms("ب");
        assert_eq!(s[0].unwrap().ch, '\u{fe8f}');

        let s = forms("بب");
        assert_eq!(s[0].unwrap().ch, '\u{fe91}');
        assert_eq!(s[1].unwrap().ch, '\u{fe90}');

        let s = forms("ببب");
        assert_eq!(s[1].unwrap().ch, '\u{fe92}');
    }

    #[test]
    fn right_joining_letters_do_not_join_left() {
        let s = forms("اب");
        assert_eq!(s[0].unwrap().ch, '\u{fe8d}');
        assert_eq!(s[1].unwrap().ch, '\u{fe8f}');
    }

    #[test]
    fn lam_alef_ligatures() {
        let s = forms("لا");
        assert_eq!(s[0].unwrap().ch, '\u{fefb}');
        assert_eq!(s[0].unwrap().len, 2);
        assert!(s[1].is_none());

        let s = forms("بلا");
        assert_eq!(s[1].unwrap().ch, '\u{fefc}');
    }

    #[test]
    fn marks_are_transparent_for_joining() {
        let s = forms("بَب");
        assert_eq!(s[0].unwrap().ch, '\u{fe91}');
        assert_eq!(s[2].unwrap().ch, '\u{fe90}');
    }
}
