//! The XSD-regex dialect translator — the Rust mirror of
//! lutaml/moxml's `Moxml::XSD::SchemaRules::XsdRegex` (#464/#496).
//!
//! Bounded to the dialect subset the facade vouches for (Datatypes
//! Appendix F): block escapes (`\p{Is…}`/`\P{Is…}`) map to the
//! `In`-prefixed form (a few Unicode-renamed blocks carry an alias);
//! general categories and the single-character escapes pass through;
//! `\i`, `\c` and class subtraction are NOT translated. Anything
//! unrecognized — or a translation the host regex engine rejects —
//! returns `None`: the engine's answer then stands.

use regex::Regex;

const BLOCK_ALIASES: [(&str, &str); 3] = [
    ("Greek", "GreekandCoptic"),
    (
        "CombiningMarksforSymbols",
        "CombiningDiacriticalMarksforSymbols",
    ),
    ("Turkish", "Latin-1Supplement"),
];

const PASS_THROUGH_ESCAPES: [&str; 14] = [
    "n", "r", "t", "\\", ".", "|", "?", "*", "+", "(", ")", "[", "]", "{",
];

/// Translates an XSD pattern to the host regex dialect, anchored to
/// the whole string. `None` when the pattern uses a construct outside
/// the translated subset or the host engine rejects the translation.
pub fn translate(pattern: &str) -> Option<Regex> {
    let translated = scan(pattern)?;
    Regex::new(&format!(r"\A(?:{translated})\z")).ok()
}

fn scan(pattern: &str) -> Option<String> {
    let mut out = String::with_capacity(pattern.len() + 8);
    let mut index = 0usize;
    let chars: Vec<char> = pattern.chars().collect();
    while index < chars.len() {
        if chars[index] == '\\' {
            index += escape(&chars, index, &mut out)?;
        } else {
            // Class subtraction is outside the translated subset.
            if chars[index] == '-' && chars.get(index + 1) == Some(&'[') {
                return None;
            }
            out.push(chars[index]);
            index += 1;
        }
    }
    Some(out)
}

/// Appends the translation of the escape at `index` to `out` and
/// returns the characters consumed, or `None` when unrecognized.
fn escape(chars: &[char], index: usize, out: &mut String) -> Option<usize> {
    let kind = *chars.get(index + 1)?;
    match kind {
        'p' | 'P' => {
            // \p{IsName} / \P{IsName}: the close brace must exist and
            // the body must name a block in the range table. The host
            // regex engine has no In-prefixed block names, so the
            // translation emits the block's own character range
            // (negated for \P). Blocks outside the table return None —
            // the narrower surface, loudly. Non-Is bodies are the
            // host's own faces (general categories) and pass through
            // verbatim.
            let close = chars[index + 2..].iter().position(|c| *c == '}')? + index + 2;
            let body: String = chars[index + 3..close].iter().collect();
            if let Some(stripped) = body.strip_prefix("Is") {
                let mapped = block_alias(stripped).unwrap_or(stripped);
                let (low, high) = block_range(mapped)?;
                out.push('[');
                if kind == 'P' {
                    out.push('^');
                }
                out.push_str(&format!("\\u{{{:04X}}}-\\u{{{:04X}}}}}", low, high));
                out.push(']');
            } else {
                out.push('\\');
                out.push(kind);
                out.push('{');
                out.push_str(&body);
                out.push('}');
            }
            Some(close - index + 1)
        }
        _ => {
            let kind_str = kind.to_string();
            if !PASS_THROUGH_ESCAPES.contains(&kind_str.as_str()) {
                return None;
            }
            out.push('\\');
            out.push(kind);
            Some(2)
        }
    }
}

/// Unicode block ranges the translator serves. The Ruby side
/// validates against the host engine's full block set; this table is
/// the Rust side's (extensible) coverage — blocks outside it return
/// None and keep the engine's answer.
const BLOCK_RANGES: &[(&str, u32, u32)] = &[
    ("BasicLatin", 0x0000, 0x007F),
    ("Latin-1Supplement", 0x0080, 0x00FF),
    ("LatinExtended-A", 0x0100, 0x017F),
    ("LatinExtended-B", 0x0180, 0x024F),
    ("IPAExtensions", 0x0250, 0x02AF),
    ("SpacingModifierLetters", 0x02B0, 0x02FF),
    ("CombiningDiacriticalMarks", 0x0300, 0x036F),
    ("Greek", 0x0370, 0x03FF),
    ("GreekandCoptic", 0x0370, 0x03FF),
    ("Cyrillic", 0x0400, 0x04FF),
    ("Hebrew", 0x0590, 0x05FF),
    ("Arabic", 0x0600, 0x06FF),
    ("Armenian", 0x0530, 0x058F),
    ("GeneralPunctuation", 0x2000, 0x206F),
    ("CurrencySymbols", 0x20A0, 0x20CF),
    ("LetterlikeSymbols", 0x2100, 0x214F),
    ("NumberForms", 0x2150, 0x218F),
    ("MathematicalOperators", 0x2200, 0x22FF),
    ("Hiragana", 0x3040, 0x309F),
    ("Katakana", 0x30A0, 0x30FF),
    ("Bopomofo", 0x3100, 0x312F),
    ("CJKUnifiedIdeographs", 0x4E00, 0x9FFF),
    ("HangulSyllables", 0xAC00, 0xD7AF),
    ("CombiningDiacriticalMarksforSymbols", 0x20D0, 0x20FF),
];

fn block_range(name: &str) -> Option<(u32, u32)> {
    BLOCK_RANGES
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, low, high)| (*low, *high))
}

fn block_alias(body: &str) -> Option<&'static str> {
    BLOCK_ALIASES
        .iter()
        .find(|(from, _)| *from == body)
        .map(|(_, to)| *to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_blocks_and_categories() {
        let r = translate(r"\p{IsBasicLatin}+").unwrap();
        assert!(r.is_match("abc"));
        assert!(!r.is_match("\u{80}"));

        let r = translate(r"\P{IsLatin-1Supplement}").unwrap();
        assert!(r.is_match("a"));
        assert!(!r.is_match("é"));

        let r = translate(r"\p{IsGreek}").unwrap();
        assert!(r.is_match("α"));
        assert!(!r.is_match("a"));

        assert!(translate(r"\p{L}").unwrap().is_match("a"));
        assert!(!translate(r"\p{L}").unwrap().is_match("1"));
    }

    #[test]
    fn refuses_out_of_subset_constructs() {
        assert!(translate(r"\i\c*").is_none());
        assert!(translate(r"[\p{IsBasicLatin}-[abc]]+").is_none());
        assert!(translate(r"\p{IsNoSuchBlock}").is_none());
        assert!(translate(r"\d{1,3}").is_none());
    }
}
