#![doc = include_str!("../README.md")]
use crate::{elements::SsmlElement, parser::Span};
use elements::ParsedElement;
use std::fmt;
use std::ops::FnMut;

// Public re-export
pub use crate::parser::parse_ssml;

pub mod elements;
pub mod parser;

/// Holds parsed SSML string with the text minus tags and the tag information
#[derive(Clone, Debug)]
pub struct Ssml {
    /// Text with all tags removed
    text: String,
    /// Vector of tags stored in a depth first search ordering
    pub(crate) tags: Vec<Span>,
    /// Simple parse tree to represent the XML document structure
    pub(crate) event_log: ParserLog,
}

/// After applying a transformation to SSML writes out the new SSML string and also the
/// text to be processed by a speech synthesiser. Assumes all text in custom tags is synthesisable.
#[derive(Clone, Debug)]
pub struct TransformedSsml {
    /// Generated SSML String after transformation
    pub ssml_string: String,
    /// Synthesisable text after the transformation
    pub synthesisable_text: String,
}

/// List of XML events representing the document in the order it was parsed.
type ParserLog = Vec<ParserLogEvent>;

/// Represents the XML document structure
#[derive(Clone, Debug)]
pub(crate) enum ParserLogEvent {
    /// Text within tags with the start and end character indices
    Text((usize, usize)),
    /// An XML open tag
    Open(ParsedElement),
    /// An XML close tag
    Close(ParsedElement),
    /// An empty XML i.e. `<break/>`
    Empty(ParsedElement),
}

/// An owned version of the parser event, this is created to allow for the asynchronous map
/// transform of the tree without worrying about ownership issues so will take an owned copy of
/// substrings of the tag-less text.
#[derive(Clone, Debug)]
pub enum ParserEvent {
    /// Some text within a pair of XML tags
    Text(String),
    /// An XML open tag
    Open(ParsedElement),
    /// An XML close tag
    Close(ParsedElement),
    /// An empty XML i.e. `<break/>`
    Empty(ParsedElement),
}

/// This trait defines a function used to transform the ssml when asynchronous operations are
/// involved.
#[cfg(feature = "async")]
#[async_trait::async_trait]
pub trait AsyncSsmlTransformer {
    /// Can be thought of as an asynchronous filter_map. Given a `ParserEvent` it will either
    /// return a `ParserEvent` to be inserted into the stream or a `None` to remove the event from
    /// the event stream. Self is mutable to allow for internal tracking of values.
    async fn apply(&mut self, event: ParserEvent) -> Option<ParserEvent>;
}

impl fmt::Display for ParserEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(text) => write!(f, "{}", quick_xml::escape::escape(&text)),
            Self::Open(element) => {
                let name: SsmlElement = element.into();
                write!(f, "<{}{}>", name, element.attribute_string())
            }
            Self::Close(element) => {
                let name: SsmlElement = element.into();
                write!(f, "</{}>", name)
            }
            Self::Empty(element) => {
                let name: SsmlElement = element.into();
                write!(f, "<{}{}/>", name, element.attribute_string())
            }
        }
    }
}

impl Ssml {
    /// Gets a version of the text with all the SSML tags stripped
    pub fn get_text(&self) -> &str {
        &self.text
    }

    /// From a given span with start/end characters return the text within that span.
    ///
    /// # Panics
    ///
    /// Will panic if span exceeds the bounds of the text.
    pub fn get_text_from_span(&self, span: &Span) -> &str {
        assert!(span.end <= self.text.len() && span.end >= span.start);
        &self.text[span.start..span.end]
    }

    /// Get an iterator over the SSML tags - traversed depth first.
    pub fn tags(&self) -> impl Iterator<Item = &Span> {
        self.tags.iter()
    }

    /// Write out the SSML text again - mainly used for testing correctness of implementation.
    pub fn write_ssml(&self) -> String {
        let mut ssml_str = String::new();

        use ParserLogEvent::*;
        for event in self.event_log.iter() {
            ssml_str.push_str(&match event {
                Text(span) => {
                    let (start, end) = *span;
                    quick_xml::escape::escape(&self.text[start..end]).to_string()
                }
                Open(element) => {
                    let name: SsmlElement = element.into();
                    format!("<{}{}>", name, element.attribute_string())
                }
                Close(element) => {
                    let name: SsmlElement = element.into();
                    format!("</{}>", name)
                }
                Empty(element) => {
                    let name: SsmlElement = element.into();
                    format!("<{}{}/>", name, element.attribute_string())
                }
            });
        }

        ssml_str
    }

    /// For each parser event to write out apply a transformation to it or return None if it should
    /// be filtered out. It is up to the implementor to make sure that if an open tag is removed
    /// the corresponding close tag is removed as well.
    ///
    /// TODO this doesn't track if there are tags where inner text shouldn't be synthesised so
    /// certain transformations will lead to synthesisable_text being incorrect.
    pub fn write_ssml_with_transform<F>(&self, mut f: F) -> TransformedSsml
    where
        F: FnMut(ParserEvent) -> Option<ParserEvent>,
    {
        let mut ssml_string = String::new();
        let mut synthesisable_text = String::new();

        use ParserLogEvent::*;
        for event in self.event_log.iter().cloned() {
            let new_event = match event {
                Text((start, end)) => f(ParserEvent::Text(self.text[start..end].to_string())),
                Open(element) => f(ParserEvent::Open(element)),
                Close(element) => f(ParserEvent::Close(element)),
                Empty(element) => f(ParserEvent::Empty(element)),
            };
            if let Some(new_event) = new_event {
                let string = new_event.to_string();
                ssml_string.push_str(&string);
                if let ParserEvent::Text(t) = new_event {
                    synthesisable_text.push_str(&t);
                }
            }
        }
        TransformedSsml {
            ssml_string,
            synthesisable_text,
        }
    }

    /// Turns the SSML document into a stream of events with open/close tags, text and empty
    /// elements. This will not filter out text that shouldn't be synthesised so it's on the user
    /// to keep track of this.
    pub fn event_iter(&self) -> impl Iterator<Item = ParserEvent> + '_ {
        self.event_log.iter().cloned().map(|x| match x {
            ParserLogEvent::Text((start, end)) => {
                ParserEvent::Text(self.text[start..end].to_string())
            }
            ParserLogEvent::Open(elem) => ParserEvent::Open(elem),
            ParserLogEvent::Close(elem) => ParserEvent::Close(elem),
            ParserLogEvent::Empty(elem) => ParserEvent::Empty(elem),
        })
    }

    /// For each parser event to write out apply a transformation to it or return None if it should
    /// be filtered out. It is up to the implementor to make sure that if an open tag is removed
    /// the corresponding close tag is removed as well.
    ///
    /// TODO this doesn't track if there are tags where inner text shouldn't be synthesised so
    /// certain transformations will lead to synthesisable_text being incorrect.
    #[cfg(feature = "async")]
    pub async fn async_write_ssml_with_transform(
        self,
        mut f: impl AsyncSsmlTransformer,
    ) -> TransformedSsml {
        let mut ssml_string = String::new();
        let mut synthesisable_text = String::new();

        use ParserLogEvent::*;
        for event in self.event_log.iter().cloned() {
            let new_event = match event {
                Text(span) => {
                    let (start, end) = span;
                    f.apply(ParserEvent::Text(self.text[start..end].to_string()))
                        .await
                }
                Open(element) => f.apply(ParserEvent::Open(element)).await,
                Close(element) => f.apply(ParserEvent::Close(element)).await,
                Empty(element) => f.apply(ParserEvent::Empty(element)).await,
            };
            if let Some(new_event) = new_event {
                let string = new_event.to_string();
                ssml_string.push_str(&string);
                if let ParserEvent::Text(t) = new_event {
                    synthesisable_text.push_str(&t);
                }
            }
        }
        TransformedSsml {
            ssml_string,
            synthesisable_text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_ssml;
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;
    use quick_xml::writer::Writer;
    use std::io::Cursor;

    #[test]
    fn basic_ssml_writing() {
        let ssml = r#"
        <speak version="1.0" xml:lang="string" foo="&amp;" xmlns="http://www.w3.org/2001/10/synthesis" xmlns:mstts="https://www.w3.org/2001/mstts">
            <mstts:backgroundaudio fadein="string" fadeout="string" src="string" volume="string"/>
            <voice name="string">
                <bookmark mark="string"/>
                <break strength="medium" time="5s"/>
                <emphasis level="reduced"/>
                <lang xml:lang="string"/>
                <lexicon uri="string" xml:id="some_id"/>
                <math xmlns="http://www.w3.org/1998/Math/MathML"/>
                <mstts:express-as role="string" style="string" styledegree="value"/>
                <mstts:silence type="string" value="string"/>
                <mstts:viseme type="string &amo;"/>
                <p>Some speech! &amp; With correct escaping on text, hopefully. </p>
                <phoneme ph="string" alphabet="string"/>
                <prosody pitch="2.2Hz" contour="(0%,+20Hz) (10%,+30Hz) (40%,+10Hz)" range="-2Hz" rate="20%" volume="2dB"/>
                <s/>
                <say-as interpret-as="string" format="string" detail="string"/>
                <sub alias="correct escaping of attributes &amp;"> Keep me here </sub>
            </voice>
        </speak>        
        "#;

        let rewritten = parse_ssml(ssml).unwrap().write_ssml();

        let mut reader = Reader::from_str(ssml);
        reader.trim_text(true);
        let mut writer = Writer::new(Cursor::new(vec![]));

        loop {
            match reader.read_event().unwrap() {
                Event::Eof => break,
                e => writer.write_event(e).unwrap(),
            }
        }

        let ssml = String::from_utf8(writer.into_inner().into_inner()).unwrap();

        let mut reader = Reader::from_str(&rewritten);
        reader.trim_text(true);
        let mut writer = Writer::new(Cursor::new(vec![]));

        loop {
            match reader.read_event().unwrap() {
                Event::Eof => break,
                e => writer.write_event(e).unwrap(),
            }
        }

        let rewritten_trimmed = String::from_utf8(writer.into_inner().into_inner()).unwrap();

        println!("Original:");
        println!("{}", ssml);
        println!("Rewritten:");
        println!("{}", rewritten_trimmed);

        assert!(&ssml == &rewritten_trimmed);
    }

    #[test]
    fn ssml_transformation() {
        let ssml = r#"
        <speak>
            <mstts:backgroundaudio fadein="string" fadeout="string" src="string" volume="string"/>
            <voice name="string">
                <break strength="medium" time="5s"/>
                <emphasis level="reduced"/>
                <lang xml:lang="string"/>
                <lexicon uri="string" xml:id="some_id"/>
                <mstts:express-as role="string" style="string" styledegree="value"/>
                <p>Some speech! &amp; With correct escaping on text, hopefully. </p>
                <phoneme ph="string" alphabet="string"/>
                <prosody pitch="2.2Hz" contour="(0%,+20Hz) (10%,+30Hz) (40%,+10Hz)" range="-2Hz" rate="20%" volume="2dB"/>
            </voice>
        </speak>        
        "#;

        let ssml = parse_ssml(ssml).unwrap();
        // Now here we want to strip away the mstts tags and replace some text to be said. And then
        // we'll reparse and make sure things seem sane

        let transform = |elem| match &elem {
            ParserEvent::Open(element)
            | ParserEvent::Close(element)
            | ParserEvent::Empty(element) => {
                if matches!(element, ParsedElement::Custom(_)) {
                    None
                } else {
                    Some(elem)
                }
            }
            ParserEvent::Text(txt) => {
                let txt = txt.replace("hopefully", "definitely");
                Some(ParserEvent::Text(txt))
            }
        };

        let transformed = ssml.write_ssml_with_transform(transform);
        assert_eq!(
            transformed.synthesisable_text.trim(),
            "Some speech! & With correct escaping on text, definitely."
        );
        assert!(!transformed.ssml_string.contains("mstts:backgroundaudio"));
        assert!(!transformed.ssml_string.contains("mstts:express-as"));
        assert!(transformed.ssml_string.contains("prosody"));

        // and hopefully our ssml is still valid:
        parse_ssml(&transformed.ssml_string).unwrap();
    }
}
