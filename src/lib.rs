use elements::ParsedElement;

use crate::{elements::SsmlElement, parser::Span};
/// Valid SSML:
///
/// ```xml
/// <speak>
/// Here are <say-as interpret-as="characters">SSML</say-as> samples.
/// I can pause <break time="3s"/>.
/// I can play a sound
/// <audio src="https://www.example.com/MY_MP3_FILE.mp3">didn't get your MP3 audio file</audio>.
/// I can speak in cardinals. Your number is <say-as interpret-as="cardinal">10</say-as>.
/// Or I can speak in ordinals. You are <say-as interpret-as="ordinal">10</say-as> in line.
/// Or I can even speak in digits. The digits for ten are <say-as interpret-as="characters">10</say-as>.
/// I can also substitute phrases, like the <sub alias="World Wide Web Consortium">W3C</sub>.
/// Finally, I can speak a paragraph with two sentences.
/// <p><s>This is sentence one.</s><s>This is sentence two.</s></p
/// </speak>
/// ```
pub mod elements;
pub mod parser;

#[derive(Clone, Debug)]
pub struct Ssml {
    text: String,
    pub(crate) tags: Vec<Span>,
    pub(crate) event_log: ParserLog,
}

type ParserLog = Vec<ParserLogEvent>;

#[derive(Clone, Debug)]
pub(crate) enum ParserLogEvent {
    Text((usize, usize)),
    Open(ParsedElement),
    Close(ParsedElement),
    Empty(ParsedElement),
}

impl Ssml {
    pub fn get_text(&self) -> &str {
        &self.text
    }

    pub fn get_text_from_span(&self, span: &Span) -> &str {
        assert!(span.end <= self.text.len() && span.end >= span.start);
        &self.text[span.start..span.end]
    }

    pub fn tags(&self) -> impl Iterator<Item = &Span> {
        self.tags.iter()
    }

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
}

#[cfg(test)]
mod tests {
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
                <audio fetchhint="prefetch" src="string"/>
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
}
