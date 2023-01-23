use crate::parser::Span;
use indexmap::IndexMap;
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
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::io;
use std::time::Duration;

pub mod elements;
pub mod parser;

#[derive(Clone, Debug)]
pub struct Ssml {
    text: String,
    pub tags: Vec<Span>,
}

impl Ssml {
    pub fn get_text(&self) -> &str {
        &self.text
    }
}
