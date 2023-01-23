use crate::elements::*;
use crate::*;
use anyhow::{bail, Result};
use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use std::collections::HashMap;
use std::io;
use std::str::from_utf8;
use std::str::FromStr;
use std::time::Duration;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub element: ParsedElement,
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
    //let mut prepared_tags = vec![]; todo put finished tags here
    loop {
        match reader.read_event()? {
            Event::Start(e) if e.local_name().as_ref() == b"speak" => {
                // TODO how to handle nested speech tags
                if !has_started {
                    text_buffer.clear();
                }
                has_started = true;
                let span = Span {
                    start: text_buffer.len(),
                    end: text_buffer.len(),
                    element: parse_speak(e, &reader)?,
                };
                open_tags.push((SsmlElement::Speak, span));
                // Okay we have speech top level here.
                //todo!();
            }
            Event::Start(e) => {
                if has_started {
                    if !text_buffer.ends_with(char::is_whitespace)
                        && matches!(e.local_name().as_ref(), b"s" | b"p")
                    {
                        // Need to add in a space as they're using tags instead
                        text_buffer.push(' ');
                    }
                    let (ty, element) = parse_element(e, &reader)?;
                    let new_span = Span {
                        start: text_buffer.len(),
                        end: text_buffer.len(),
                        element,
                    };
                    open_tags.push((ty, new_span));
                    // We need attributes (for some things), a
                }
            }
            Event::Comment(_)
            | Event::CData(_)
            | Event::Decl(_)
            | Event::PI(_)
            | Event::DocType(_) => continue,
            Event::Eof => break,
            Event::Text(e) => {
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
                    let (_, mut span) = open_tags.remove(open_tags.len() - 1);
                    span.end = text_buffer.len();
                    tags.insert(0, span);
                    if ssml_elem == SsmlElement::Speak && open_tags.is_empty() {
                        break;
                    }
                }
            }
            e => {
                //panic!("Unexpected event: {:?}", e);
            }
        }
    }
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
        SsmlElement::Lexicon => ParsedElement::Lexicon,
        SsmlElement::Lookup => ParsedElement::Lookup,
        SsmlElement::Meta => ParsedElement::Meta,
        SsmlElement::Metadata => ParsedElement::Metadata,
        SsmlElement::Paragraph => ParsedElement::Paragraph,
        SsmlElement::Sentence => ParsedElement::Sentence,
        SsmlElement::Token => ParsedElement::Token,
        SsmlElement::Word => ParsedElement::Word,
        SsmlElement::SayAs => ParsedElement::SayAs,
        SsmlElement::Phoneme => ParsedElement::Phoneme,
        SsmlElement::Sub => ParsedElement::Sub,
        SsmlElement::Lang => ParsedElement::Lang,
        SsmlElement::Voice => ParsedElement::Voice,
        SsmlElement::Emphasis => ParsedElement::Emphasis,
        SsmlElement::Break => ParsedElement::Break,
        SsmlElement::Prosody => ParsedElement::Prosody,
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
    let lang = elem.try_get_attribute("lang")?;
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

fn parse_paragraph<R: io::BufRead>(reader: &mut Reader<R>) -> Result<()> {
    todo!()
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
}
