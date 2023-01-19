use crate::*;
use anyhow::{bail, Result};
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use std::io;
use std::time::Duration;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

pub fn parse_ssml(ssml: &str) -> Result<Ssml> {
    let mut reader = Reader::from_str(ssml);
    reader.check_end_names(true);
    let mut has_started = false;
    let mut text_buffer = String::new();
    loop {
        match reader.read_event()? {
            Event::Start(e) if e.local_name().as_ref() == b"speak" => {
                // TODO how to handle nested speech tags
                if !has_started {
                    text_buffer.clear();
                }
                has_started = true;
                // Okay we have speech top level here.
                //todo!();
            }
            Event::Start(e) => {
                if has_started
                    && !text_buffer.ends_with(char::is_whitespace)
                    && matches!(e.local_name().as_ref(), b"s" | b"p")
                {
                    // Need to add in a space as they're using tags instead
                    text_buffer.push(' ');
                }
            }
            Event::Comment(_)
            | Event::CData(_)
            | Event::Decl(_)
            | Event::PI(_)
            | Event::DocType(_) => continue,
            Event::Eof => break,
            Event::Text(e) => {
                // We're attaching no meaning to repeated whitespace, but things like space at end
                // of text and line-breaks are word delimiters and we want to keep at least one in
                // if there are repeated. But don't want half our transcript to be formatting
                // induced whitespace
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
            }
            Event::End(e) => {
                if e.local_name().as_ref() == b"speak" {
                    break;
                }
            }
            e => {
                //panic!("Unexpected event: {:?}", e);
            }
        }
    }
    Ok(Ssml { text: text_buffer })
}

fn parse_speak<R: io::BufRead>(reader: &mut Reader<R>) -> Result<()> {
    todo!()
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
