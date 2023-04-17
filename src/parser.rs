use crate::elements::*;
use crate::*;
use anyhow::{bail, Context, Result};
use lazy_static::lazy_static;
use mediatype::MediaTypeBuf;
use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use regex::Regex;
use std::cmp::{Ord, Ordering};
use std::collections::HashMap;
use std::io;
use std::str::from_utf8;
use std::str::FromStr;
use std::time::Duration;

/// Shows a region of the cleaned transcript which an SSML element applies to.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    /// This is the index of span's start (inclusive) in terms of unicode scalar values - not bytes
    /// or graphemes
    pub start: usize,
    /// This is the of span's end (exclusive) in terms of unicode scalar values - not bytes
    /// or graphemes
    pub end: usize,
    /// The element contained within this span
    pub element: ParsedElement,
}

impl Span {
    /// Returns true if a span is contained within another span. This only takes advantage of the
    /// start and end indexes. Other constraints such as the fact the parser returns spans in order
    /// they're seen need to be used in combination to see if this _really contains_ the other
    /// span. So if you're going over the list in order you can rely on this but if you've
    /// rearranged the tag list it may not hold true.
    ///
    /// This does handle tags which can't contain other tags. So `<break/><break/>` will appear
    /// with the same start and end. However break has to be an empty tag. This function will
    /// return false. Whereas `<s/><s/>` will return true as a sentence can contain other tags. In
    /// future as a sentence cannot contain a sentence this may return false.
    pub fn maybe_contains(&self, other: &Self) -> bool {
        self.element.can_contain(&other.element)
            && (self.start <= other.start && self.end >= other.end)
    }
}

impl Eq for Span {}

impl Ord for Span {
    fn cmp(&self, other: &Self) -> Ordering {
        // We want spans that start earlier to be orderered sooner, but if both spans start in same
        // location then the one with the further ahead end is the later one
        match self.start.cmp(&other.start) {
            Ordering::Equal => other.end.cmp(&self.end),
            ord => ord,
        }
    }
}

impl PartialOrd for Span {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// We're attaching no meaning to repeated whitespace, but things like space at end
/// of text and line-breaks are word delimiters and we want to keep at least one in
/// if there are repeated. But don't want half our transcript to be formatting
/// induced whitespace.
fn push_text(e: BytesText, text_buffer: &mut String) -> Result<()> {
    let ends_in_whitespace = text_buffer.ends_with(char::is_whitespace);
    let text = e.unescape()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        if !(text_buffer.is_empty() || ends_in_whitespace) {
            text_buffer.push(' ');
        }
    } else {
        if !ends_in_whitespace && text.starts_with(char::is_whitespace) {
            text_buffer.push(' ');
        }
        let mut first = true;
        for line in trimmed.lines() {
            if !first {
                text_buffer.push(' ');
            }
            text_buffer.push_str(line.trim());
            first = false;
        }
        if text.ends_with(char::is_whitespace) {
            text_buffer.push(' ');
        }
    }
    Ok(())
}

pub fn parse_ssml(ssml: &str) -> Result<Ssml> {
    let mut reader = Reader::from_str(ssml);
    reader.check_end_names(true);
    let mut has_started = false;
    let mut text_buffer = String::new();
    let mut open_tags = vec![];
    let mut tags = vec![];
    loop {
        match reader.read_event()? {
            Event::Start(e) if e.local_name().as_ref() == b"speak" => {
                if !has_started {
                    text_buffer.clear();
                } else {
                    bail!("Speak element cannot be placed inside a Speak");
                }
                has_started = true;
                let span = Span {
                    start: text_buffer.chars().count(),
                    end: text_buffer.chars().count(),
                    element: parse_speak(e, &reader)?,
                };
                open_tags.push((SsmlElement::Speak, tags.len(), span));
            }
            Event::Start(e) => {
                // TODO implement ordering constraints:
                //
                // The meta, metadata and lexicon elements must occur before all other elements and text
                // contained within the root speak element. There are no other ordering constraints on the
                // elements in this specification.
                if has_started {
                    if !(text_buffer.is_empty() || text_buffer.ends_with(char::is_whitespace))
                        && matches!(e.local_name().as_ref(), b"s" | b"p")
                    {
                        // Need to add in a space as they're using tags instead
                        text_buffer.push(' ');
                    }
                    let (ty, element) = parse_element(e, &reader)?;
                    let new_span = Span {
                        start: text_buffer.chars().count(),
                        end: text_buffer.chars().count(),
                        element,
                    };
                    match open_tags.last().map(|x| &x.0) {
                        Some(open_type) if !open_type.can_contain(&ty) => {
                            bail!("{:?} cannot be placed inside {:?}", ty, open_type)
                        }
                        _ => {}
                    }
                    open_tags.push((ty, tags.len(), new_span));
                }
            }
            Event::Comment(_)
            | Event::CData(_)
            | Event::Decl(_)
            | Event::PI(_)
            | Event::DocType(_) => continue,
            Event::Eof => break,
            Event::Text(e) => {
                // TODO we should stop text contents of tags where insides shouldn't be added to
                // synthesis output from entering our text buffer
                push_text(e, &mut text_buffer)?;
            }
            Event::End(e) => {
                let local_name = e.local_name();
                let name = from_utf8(local_name.as_ref())?;
                if open_tags.is_empty() {
                    bail!(
                        "Invalid SSML close tag '{}' presented without open tag.",
                        name
                    );
                }
                let ssml_elem = SsmlElement::from_str(name).unwrap();
                if ssml_elem != open_tags[open_tags.len() - 1].0 {
                    // We have a close tag without an open!
                } else {
                    // Okay time to close and remove tag
                    let (_, pos, mut span) = open_tags.remove(open_tags.len() - 1);
                    span.end = text_buffer.chars().count();
                    tags.insert(pos, span);
                    if ssml_elem == SsmlElement::Speak && open_tags.is_empty() {
                        break;
                    }
                }
            }
            Event::Empty(e) => {
                let (_, element) = parse_element(e, &reader)?;
                let span = Span {
                    start: text_buffer.chars().count(),
                    end: text_buffer.chars().count(),
                    element,
                };
                tags.push(span);
            }
        }
    }
    tags.sort();
    Ok(Ssml {
        text: text_buffer,
        tags,
    })
}

fn parse_element<R: io::BufRead>(
    elem: BytesStart,
    reader: &Reader<R>,
) -> Result<(SsmlElement, ParsedElement)> {
    let local_name = elem.local_name();
    let name = from_utf8(local_name.as_ref())?;
    let elem_type = SsmlElement::from_str(name).unwrap();

    let res = match elem_type {
        SsmlElement::Speak => parse_speak(elem, reader)?,
        SsmlElement::Lexicon => parse_lexicon(elem, reader)?,
        SsmlElement::Lookup => parse_lookup(elem, reader)?,
        SsmlElement::Meta => parse_meta(elem, reader)?,
        SsmlElement::Metadata => ParsedElement::Metadata,
        SsmlElement::Paragraph => ParsedElement::Paragraph,
        SsmlElement::Sentence => ParsedElement::Sentence,
        SsmlElement::Token => ParsedElement::Token,
        SsmlElement::Word => ParsedElement::Word,
        SsmlElement::SayAs => parse_say_as(elem, reader)?,
        SsmlElement::Phoneme => parse_phoneme(elem, reader)?,
        SsmlElement::Sub => ParsedElement::Sub,
        SsmlElement::Lang => ParsedElement::Lang,
        SsmlElement::Voice => ParsedElement::Voice,
        SsmlElement::Emphasis => parse_emphasis(elem, reader)?,
        SsmlElement::Break => parse_break(elem, reader)?,
        SsmlElement::Prosody => parse_prosody(elem, reader)?,
        SsmlElement::Audio => ParsedElement::Audio,
        SsmlElement::Mark => ParsedElement::Mark,
        SsmlElement::Description => ParsedElement::Description,
        SsmlElement::Custom(ref s) => {
            let mut attributes = HashMap::new();
            for attr in elem.attributes() {
                let attr = attr?;
                attributes.insert(
                    String::from_utf8(attr.key.0.to_vec())?,
                    String::from_utf8(attr.value.to_vec())?,
                );
            }
            ParsedElement::Custom((s.to_string(), attributes))
        }
    };

    Ok((elem_type, res))
}

fn parse_speak<R: io::BufRead>(elem: BytesStart, reader: &Reader<R>) -> Result<ParsedElement> {
    let lang = elem
        .try_get_attribute("lang")?
        .or_else(|| elem.try_get_attribute("xml:lang").unwrap_or_default());
    let lang = if let Some(lang) = lang {
        Some(lang.decode_and_unescape_value(reader)?.to_string())
    } else {
        None
    };
    let base = elem.try_get_attribute("base")?;
    let base = if let Some(base) = base {
        Some(base.decode_and_unescape_value(reader)?.to_string())
    } else {
        None
    };
    let on_lang_failure = elem.try_get_attribute("nolangfailure")?;
    let on_lang_failure = if let Some(lang) = on_lang_failure {
        Some(lang.decode_and_unescape_value(reader)?.to_string())
    } else {
        None
    };
    Ok(ParsedElement::Speak(SpeakAttributes {
        lang,
        base,
        on_lang_failure,
    }))
}

fn parse_lexicon<R: io::BufRead>(elem: BytesStart, reader: &Reader<R>) -> Result<ParsedElement> {
    let xml_id = elem
        .try_get_attribute("xml:id")?
        .context("xml:id attribute is required with a lexicon element")?
        .decode_and_unescape_value(reader)?
        .to_string();

    let uri: http::Uri = elem
        .try_get_attribute("uri")?
        .context("uri attribute is required with a lexicon element")?
        .decode_and_unescape_value(reader)?
        .to_string()
        .parse()?;

    lazy_static! {
        static ref TIME_RE: Regex = Regex::new(r"^\+?((?:\d*\.)?\d)+(s|ms)$").unwrap();
    }

    let fetchtimeout = match elem.try_get_attribute("fetchtimeout")? {
        Some(fetchtimeout) => {
            let fetchtimeout = fetchtimeout.decode_and_unescape_value(reader)?;

            let caps = TIME_RE
                .captures(&fetchtimeout)
                .context("fetchtimeout attribute must be a valid TimeDesignation")?;

            let num_val = (&caps[1]).parse::<f32>().unwrap();

            match &caps[2] {
                "s" => Some(TimeDesignation::Seconds(num_val)),
                "ms" => Some(TimeDesignation::Milliseconds(num_val)),
                _ => unreachable!(),
            }
        }
        None => None,
    };

    let ty = match elem.try_get_attribute("type")? {
        Some(ty) => {
            let ty = ty.decode_and_unescape_value(reader)?.to_string();
            let ty = MediaTypeBuf::from_string(ty)
                .context("invalid media type for type attribute of lexicon element")?;

            Some(ty)
        }
        None => None,
    };

    Ok(ParsedElement::Lexicon(LexiconAttributes {
        uri,
        xml_id,
        fetchtimeout,
        ty,
    }))
}

fn parse_lookup<R: io::BufRead>(elem: BytesStart, reader: &Reader<R>) -> Result<ParsedElement> {
    let lookup_ref = elem
        .try_get_attribute("ref")?
        .context("ref attribute is required with a lookup element")?
        .decode_and_unescape_value(reader)?
        .to_string();

    Ok(ParsedElement::Lookup(LookupAttributes { lookup_ref }))
}

fn parse_meta<R: io::BufRead>(elem: BytesStart, reader: &Reader<R>) -> Result<ParsedElement> {
    let content = elem
        .try_get_attribute("content")?
        .context("content attribute is required with a meta element")?
        .decode_and_unescape_value(reader)?
        .to_string();

    let name = elem.try_get_attribute("name")?;
    let http_equiv = elem.try_get_attribute("http-equiv")?;

    let (name, http_equiv) = match (name, http_equiv) {
        (Some(name), None) => (
            Some(name.decode_and_unescape_value(reader)?.to_string()),
            None,
        ),
        (None, Some(http_equiv)) => (
            None,
            Some(http_equiv.decode_and_unescape_value(reader)?.to_string()),
        ),
        _ => {
            bail!("either name or http-equiv attr must be set in meta element (but not both)")
        }
    };

    Ok(ParsedElement::Meta(MetaAttributes {
        name,
        http_equiv,
        content,
    }))
}

fn parse_say_as<R: io::BufRead>(elem: BytesStart, reader: &Reader<R>) -> Result<ParsedElement> {
    // TODO: maybe rewrite the error handling in other parse functions to look like this.
    let interpret_as = elem
        .try_get_attribute("interpret-as")?
        .context("interpret-as attribute is required with a say-as element")?
        .decode_and_unescape_value(reader)?
        .to_string();

    let format = match elem.try_get_attribute("format")? {
        Some(attr) => Some(attr.decode_and_unescape_value(reader)?.to_string()),
        None => None,
    };

    let detail = match elem.try_get_attribute("detail")? {
        Some(attr) => Some(attr.decode_and_unescape_value(reader)?.to_string()),
        None => None,
    };

    Ok(ParsedElement::SayAs(SayAsAttributes {
        interpret_as,
        format,
        detail,
    }))
}

fn parse_phoneme<R: io::BufRead>(elem: BytesStart, reader: &Reader<R>) -> Result<ParsedElement> {
    let phoneme = elem.try_get_attribute("ph")?;
    let phoneme = if let Some(phoneme) = phoneme {
        let value = phoneme.decode_and_unescape_value(reader)?;
        value.to_string()
    } else {
        bail!("ph attribute is required with a phoneme element");
    };

    let alphabet = elem.try_get_attribute("alphabet")?;
    let alphabet = if let Some(alpha) = alphabet {
        let val = alpha.decode_and_unescape_value(reader)?;
        Some(PhonemeAlphabet::from_str(&val).unwrap())
    } else {
        None
    };

    Ok(ParsedElement::Phoneme(PhonemeAttributes {
        ph: phoneme,
        alphabet,
    }))
}

fn parse_break<R: io::BufRead>(elem: BytesStart, reader: &Reader<R>) -> Result<ParsedElement> {
    let strength = elem.try_get_attribute("strength")?;
    let strength = if let Some(strength) = strength {
        let value = strength.decode_and_unescape_value(reader)?;
        let value = Strength::from_str(&value)?;
        Some(value)
    } else {
        None
    };
    let time = elem.try_get_attribute("time")?;
    let time = if let Some(time) = time {
        let value = time.decode_and_unescape_value(reader)?;
        let duration = parse_duration(&value)?;
        Some(duration)
    } else {
        None
    };

    Ok(ParsedElement::Break(BreakAttributes { strength, time }))
}

fn parse_emphasis<R: io::BufRead>(elem: BytesStart, reader: &Reader<R>) -> Result<ParsedElement> {
    let level = elem.try_get_attribute("level")?;
    let level = if let Some(level) = level {
        let value = level.decode_and_unescape_value(reader)?;
        let value = EmphasisLevel::from_str(&value)?;
        Some(value)
    } else {
        None
    };

    Ok(ParsedElement::Emphasis(EmphasisAttributes { level }))
}

fn parse_prosody<R: io::BufRead>(elem: BytesStart, reader: &Reader<R>) -> Result<ParsedElement> {
    let pitch = elem.try_get_attribute("pitch")?;
    let pitch = if let Some(pitch) = pitch {
        let value = pitch.decode_and_unescape_value(reader)?;
        let value = match PitchRange::from_str(&value) {
            Ok(result) => result,
            Err(e) => bail!("Error: {}", e),
        };

        Some(value)
    } else {
        None
    };
    let contour = elem.try_get_attribute("contour")?;
    let contour = if let Some(contour) = contour {
        let value = contour.decode_and_unescape_value(reader)?;
        let value = match PitchContour::from_str(&value) {
            Ok(result) => result,
            Err(e) => bail!("Error: {}", e),
        };
        Some(value)
    } else {
        None
    };
    let range = elem.try_get_attribute("range")?;
    let range = if let Some(range) = range {
        let value = range.decode_and_unescape_value(reader)?;
        let value = match PitchRange::from_str(&value) {
            Ok(result) => result,
            Err(e) => bail!("Error: {}", e),
        };

        Some(value)
    } else {
        None
    };
    let rate = elem.try_get_attribute("rate")?;
    let rate = if let Some(rate) = rate {
        let value = rate.decode_and_unescape_value(reader)?;
        let value = match RateRange::from_str(&value) {
            Ok(result) => result,
            Err(e) => bail!("Error: {}", e),
        };

        Some(value)
    } else {
        None
    };
    let duration = elem.try_get_attribute("duration")?;
    let duration = if let Some(duration) = duration {
        let value = duration.decode_and_unescape_value(reader)?;
        let duration = parse_duration(&value)?;
        Some(duration)
    } else {
        None
    };
    let volume = elem.try_get_attribute("volume")?;
    let volume = if let Some(volume) = volume {
        let value = volume.decode_and_unescape_value(reader)?;
        let value = match VolumeRange::from_str(&value) {
            Ok(result) => result,
            Err(e) => bail!("Error: {}", e),
        };

        Some(value)
    } else {
        None
    };

    Ok(ParsedElement::Prosody(ProsodyAttributes {
        pitch,
        contour,
        range,
        rate,
        duration,
        volume,
    }))
}

fn parse_duration(duration: &str) -> Result<Duration> {
    if duration.ends_with("ms") && duration.len() > 2 {
        let time = &duration[0..(duration.len() - 2)].parse::<f32>()?;
        Ok(Duration::from_secs_f32(*time / 1000.0))
    } else if duration.ends_with("s") && duration.len() > 1 {
        let time = &duration[0..(duration.len() - 1)].parse::<f32>()?;
        Ok(Duration::from_secs_f32(*time))
    } else if duration.is_empty() {
        bail!("duration string is empty");
    } else {
        bail!("invalid time: '{}'", duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_durations() {
        assert_eq!(parse_duration("1s").unwrap(), Duration::from_secs(1));
        assert_eq!(
            parse_duration("1.5s").unwrap(),
            Duration::from_secs_f32(1.5)
        );
        assert_eq!(parse_duration("1000ms").unwrap(), Duration::from_secs(1));
        assert!(parse_duration("1s 500ms").is_err());
        assert!(parse_duration("1").is_err());
        assert!(parse_duration("five score and thirty years").is_err());
    }

    #[test]
    fn span_ordering() {
        let a = Span {
            start: 0,
            end: 10,
            element: ParsedElement::Speak(Default::default()),
        };

        let b = Span {
            start: 0,
            end: 5,
            element: ParsedElement::Speak(Default::default()),
        };

        let c = Span {
            start: 4,
            end: 5,
            element: ParsedElement::Speak(Default::default()),
        };

        let d = Span {
            start: 11,
            end: 15,
            element: ParsedElement::Speak(Default::default()),
        };

        assert!(a < b);
        assert!(b < c);
        assert!(a < c);
        assert!(a < d);
        assert!(a == a);
    }

    #[test]
    fn char_position_not_byte() {
        let unicode = parse_ssml("<speak>Let’s review a complex structure. Please note how threshold of control is calculated in this example.</speak>").unwrap();
        let ascii = parse_ssml("<speak>Let's review a complex structure. Please note how threshold of control is calculated in this example.</speak>").unwrap();

        let master_span_unicode = unicode.tags().next().unwrap();
        let master_span_ascii = ascii.tags().next().unwrap();

        assert_eq!(master_span_ascii.end, master_span_unicode.end);
        assert_eq!(master_span_ascii.end, ascii.get_text().chars().count());
    }

    #[test]
    fn span_contains() {
        let empty = parse_ssml("<speak><break/><break/></speak>").unwrap();

        assert!(empty.tags[0].maybe_contains(&empty.tags[1]));
        assert!(empty.tags[0].maybe_contains(&empty.tags[2]));
        assert!(!empty.tags[1].maybe_contains(&empty.tags[2]));

        let hello = parse_ssml("<speak>Hello <s><w>hello</w></s> world <break/></speak>").unwrap();
        assert!(hello.tags[0].maybe_contains(&hello.tags[1]));
        assert!(hello.tags[0].maybe_contains(&hello.tags[2]));
        assert!(hello.tags[0].maybe_contains(&hello.tags[3]));
        assert!(hello.tags[1].maybe_contains(&hello.tags[2]));
        assert!(!hello.tags[1].maybe_contains(&hello.tags[3]));
        assert!(!hello.tags[2].maybe_contains(&hello.tags[3]));

        let empty = parse_ssml("<speak>Hello <p></p><p></p></speak>").unwrap();
        assert!(!empty.tags[1].maybe_contains(&empty.tags[2]));

        let break_inside_custom = parse_ssml(r#"<speak><mstts:express-as style="string" styledegree="value" role="string">hello<break/> world</mstts:express-as></speak>"#).unwrap();
        assert!(break_inside_custom.tags[1].maybe_contains(&break_inside_custom.tags[2]));
    }

    #[test]
    fn reject_invalid_combos() {
        assert!(parse_ssml("<speak><speak>hello</speak></speak>").is_err());
        assert!(parse_ssml("<speak><p>hello<p>world</p></p></speak>").is_err());
    }
}
