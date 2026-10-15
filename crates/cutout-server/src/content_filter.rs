use crate::{Error, model::SpotInput};
use cutout_ride_maps::{ChargingConnector, ChargingSupply};

// This baseline filters explicit abuse before publication. Reports and moderator removal
// cover context, languages and evasions a vocabulary filter cannot reliably classify.
const BLOCKED_WORDS: &[&str] = &[
    "fuck",
    "fucks",
    "fucked",
    "fucker",
    "fuckers",
    "fucking",
    "motherfucker",
    "motherfuckers",
    "motherfucking",
    "fck",
    "fuk",
    "fukk",
    "shit",
    "shits",
    "shitty",
    "bullshit",
    "asshole",
    "assholes",
    "bitch",
    "bitches",
    "cunt",
    "cunts",
    "nigger",
    "niggers",
    "nigga",
    "niggas",
    "faggot",
    "faggots",
    "porn",
    "porno",
    "pornography",
];

pub(crate) fn validate(input: &SpotInput) -> Result<(), Error> {
    let mut text = std::iter::once(input.access_note.as_str())
        .chain(input.name.as_deref())
        .chain(input.hours.as_deref())
        .chain(
            input
                .supplies
                .iter()
                .flat_map(ChargingSupply::connectors)
                .filter_map(|connector| {
                    if let ChargingConnector::Other(label) = connector {
                        Some(label.as_str())
                    } else {
                        None
                    }
                }),
        );
    if text.any(objectionable) {
        return Err(Error::Invalid);
    }
    Ok(())
}

fn normalized_character(value: char) -> Option<char> {
    match value {
        // Ignore common invisible separators used inside otherwise recognizable words.
        '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}' | '\u{feff}' | '\u{034f}' => None,
        '\u{ff01}'..='\u{ff5e}' => char::from_u32(u32::from(value) - 0xfee0),
        '0' => Some('o'),
        '1' => Some('i'),
        '3' => Some('e'),
        '4' | '@' => Some('a'),
        '5' | '$' => Some('s'),
        '7' => Some('t'),
        _ => Some(value),
    }
}

fn objectionable(text: &str) -> bool {
    let normalized: String = text
        .chars()
        .filter_map(normalized_character)
        .flat_map(char::to_lowercase)
        .collect();
    let words: Vec<&str> = normalized
        .split(|value: char| !value.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    if words.iter().any(|word| BLOCKED_WORDS.contains(word))
        || words.windows(2).any(|pair| match pair {
            ["kill", "yourself" | "yourselves"] => true,
            _ => false,
        })
    {
        return true;
    }
    // Match punctuation/space-separated spelling without substring matches in place names.
    let mut spelled = String::new();
    for word in words {
        if word.chars().count() == 1 {
            spelled.push_str(word);
        } else {
            if BLOCKED_WORDS.contains(&spelled.as_str()) {
                return true;
            }
            spelled.clear();
        }
    }
    BLOCKED_WORDS.contains(&spelled.as_str())
}

#[cfg(test)]
mod tests {
    use super::objectionable;

    #[test]
    fn explicit_abuse_and_common_obfuscations_are_filtered_at_word_boundaries() {
        for text in [
            "FUCK!",
            "ＦＵＣＫ off",
            "f.u.c.k",
            "f u c k",
            "sh1t",
            "f\u{200b}uck",
            "kill yourself",
        ] {
            assert!(objectionable(text), "missed {text}");
        }
        for text in [
            "Scunthorpe",
            "brass socket",
            "Assembly Hall",
            "The passage",
            "shut off the breaker",
            "Dick's Sporting Goods",
        ] {
            assert!(!objectionable(text), "incorrectly filtered {text}");
        }
    }
}
