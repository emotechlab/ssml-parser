//! Handles parsing SSML input and returning our `Ssml` structure, contains a simple parse function
//! that sets up the parser with the default options and hides it as well as a parser type a user
//! can construct themselves to have more control over parsing.
use crate::elements::*;
use crate::*;
use anyhow::{bail, Context, Result};
use derive_builder::Builder;
use lazy_static::lazy_static;
use mediatype::MediaTypeBuf;
use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use regex::Regex;
use std::cmp::{Ord, Ordering};
use std::collections::BTreeMap;
use std::io;
use std::num::NonZeroUsize;
use std::str::from_utf8;
use std::str::FromStr;

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

/// SSML parser, contains options used during parsing to determine how to handle certain elements.
#[derive(Clone, Debug, Builder)]
pub struct SsmlParser {
    /// If true expand substitution elements replacing them with the text to substitute in the
    /// attribute.
    #[builder(default = "false")]
    expand_sub: bool,
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

/// Parses SSML with a default `SsmlParser`
pub fn parse_ssml(ssml: &str) -> Result<Ssml> {
    SsmlParserBuilder::default().build().unwrap().parse(ssml)
}

impl SsmlParser {
    /// Returns true if the text should be added to the text buffer. If text isn't synthesisable
    /// then it won't be entered.
    fn text_should_enter_buffer(&self, element: Option<&SsmlElement>) -> bool {
        match element {
            None => true,
            Some(elem) => {
                !(self.expand_sub && elem == &SsmlElement::Sub)
                    && elem.contains_synthesisable_text()
            }
        }
    }

    /// Parse the given SSML string
    pub fn parse(&self, ssml: &str) -> Result<Ssml> {
        let mut reader = Reader::from_str(ssml);
        reader.check_end_names(true);
        let mut has_started = false;
        let mut text_buffer = String::new();
        let mut open_tags = vec![];
        let mut tags = vec![];
        let mut event_log = vec![];

        loop {
            match reader.read_event()? {
                Event::Start(e) if e.local_name().as_ref() == b"speak" => {
                    if !has_started {
                        text_buffer.clear();
                    } else {
                        bail!("Speak element cannot be placed inside a Speak");
                    }
                    has_started = true;

                    let element = parse_speak(e, &reader)?;
                    event_log.push(ParserLogEvent::Open(element.clone()));

                    let span = Span {
                        start: text_buffer.chars().count(),
                        end: text_buffer.chars().count(),
                        element,
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
                        let (ty, element) = parse_element(e, &mut reader)?;
                        if ty == SsmlElement::Sub && self.expand_sub {
                            if let ParsedElement::Sub(attrs) = &element {
                                let text_start = text_buffer.len();
                                text_buffer.push(' ');
                                text_buffer.push_str(&attrs.alias);
                                text_buffer.push(' ');
                                let text_end = text_buffer.len();
                                event_log.push(ParserLogEvent::Text((text_start, text_end)));
                            } else {
                                unreachable!("Sub element wasn't returned for sub type");
                            }
                        } else {
                            event_log.push(ParserLogEvent::Open(element.clone()));
                            match open_tags.last().map(|x| &x.0) {
                                Some(open_type) if !open_type.can_contain(&ty) => {
                                    bail!("{:?} cannot be placed inside {:?}", ty, open_type)
                                }
                                _ => {}
                            }
                        }
                        let new_span = Span {
                            start: text_buffer.chars().count(),
                            end: text_buffer.chars().count(),
                            element,
                        };

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
                    let elem = open_tags.last().map(|x| &x.0);
                    if self.text_should_enter_buffer(elem) {
                        let text_start = text_buffer.len();
                        push_text(e, &mut text_buffer)?;
                        let text_end = text_buffer.len();
                        event_log.push(ParserLogEvent::Text((text_start, text_end)));
                    }
                }
                Event::End(e) => {
                    let name = e.name();
                    let name = from_utf8(name.as_ref())?;
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
                        if !(ssml_elem == SsmlElement::Sub && self.expand_sub) {
                            event_log.push(ParserLogEvent::Close(span.element.clone()));
                            span.end = text_buffer.chars().count();
                            tags.insert(pos, span);
                            if !(ssml_elem == SsmlElement::Speak && open_tags.is_empty()) {
                            } else {
                                break;
                            }
                        }
                    }
                }
                Event::Empty(e) => {
                    let (_, element) = parse_element(e, &mut reader)?;
                    let span = Span {
                        start: text_buffer.chars().count(),
                        end: text_buffer.chars().count(),
                        element,
                    };
                    event_log.push(ParserLogEvent::Empty(span.element.clone()));
                    tags.push(span);
                }
            }
        }
        tags.sort();
        Ok(Ssml {
            text: text_buffer,
            tags,
            event_log,
        })
    }
}

/// Parse an SSML element, this returns an `SsmlElement` as a tag to represent the SSML and the
/// `ParsedElement` with the attributes to make conditions no the ssml type easier to write.
pub(crate) fn parse_element(
    elem: BytesStart,
    reader: &mut Reader<&[u8]>,
) -> Result<(SsmlElement, ParsedElement)> {
    let name = elem.name();
    let name = from_utf8(name.as_ref())?;
    let elem_type = SsmlElement::from_str(name).unwrap();

    let res = match elem_type {
        SsmlElement::Speak => parse_speak(elem, reader)?,
        SsmlElement::Lexicon => parse_lexicon(elem, reader)?,
        SsmlElement::Lookup => parse_lookup(elem, reader)?,
        SsmlElement::Meta => parse_meta(elem, reader)?,
        SsmlElement::Metadata => ParsedElement::Metadata,
        SsmlElement::Paragraph => ParsedElement::Paragraph,
        SsmlElement::Sentence => ParsedElement::Sentence,
        SsmlElement::Token => parse_token(elem, reader)?,
        SsmlElement::Word => parse_word(elem, reader)?,
        SsmlElement::SayAs => parse_say_as(elem, reader)?,
        SsmlElement::Phoneme => parse_phoneme(elem, reader)?,
        SsmlElement::Sub => parse_sub(elem, reader)?,
        SsmlElement::Lang => parse_language(elem, reader)?,
        SsmlElement::Voice => parse_voice(elem, reader)?,
        SsmlElement::Emphasis => parse_emphasis(elem, reader)?,
        SsmlElement::Break => parse_break(elem, reader)?,
        SsmlElement::Prosody => parse_prosody(elem, reader)?,
        SsmlElement::Audio => parse_audio(elem, reader)?,
        SsmlElement::Mark => parse_mark(elem, reader)?,
        SsmlElement::Description => {
            let text = reader
                .read_text(elem.to_end().name())
                .unwrap_or_default()
                .to_string();
            ParsedElement::Description(text)
        }
        SsmlElement::Custom(ref s) => {
            let mut attributes = BTreeMap::new();
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

// TODO: handle start mark and end mark
fn parse_speak<R: io::BufRead>(elem: BytesStart, reader: &Reader<R>) -> Result<ParsedElement> {
    let version = elem.try_get_attribute("version")?;

    // Technically spec non-compliant however commercial TTS such as amazon, google and microsoft
    // don't require the version and just assume 1.1
    let version = if let Some(v) = version {
        let version = v.decode_and_unescape_value(reader)?;
        match version.as_ref() {
            "1.0" | "1.1" => (),
            v => bail!("Unsupported SSML spec version: {}", v),
        }
        version.to_string()
    } else {
        "1.1".to_string()
    };

    let lang = elem.try_get_attribute("xml:lang")?;
    let lang = if let Some(lang) = lang {
        Some(lang.decode_and_unescape_value(reader)?.to_string())
    } else {
        None
    };
    let base = elem.try_get_attribute("xml:base")?;
    let base = if let Some(base) = base {
        Some(base.decode_and_unescape_value(reader)?.to_string())
    } else {
        None
    };
    let on_lang_failure = elem.try_get_attribute("onlangfailure")?;
    let on_lang_failure = if let Some(lang) = on_lang_failure {
        let value = lang.decode_and_unescape_value(reader)?;
        Some(OnLanguageFailure::from_str(&value)?)
    } else {
        None
    };

    let mut xml_root_attrs = BTreeMap::new();
    for attr in elem.attributes() {
        let attr = attr?;

        match std::str::from_utf8(attr.key.0).unwrap() {
            "xml:base" | "xml:lang" | "onlangfailure" | "version" => continue,
            attr_name => {
                xml_root_attrs.insert(
                    String::from(attr_name),
                    String::from_utf8(attr.value.into())?,
                );
            }
        }
    }

    Ok(ParsedElement::Speak(SpeakAttributes {
        lang,
        base,
        on_lang_failure,
        version,
        xml_root_attrs,
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

    let fetch_timeout = match elem.try_get_attribute("fetchtimeout")? {
        Some(fetchtimeout) => {
            let fetchtimeout = fetchtimeout.decode_and_unescape_value(reader)?;
            Some(TimeDesignation::from_str(&fetchtimeout)?)
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
        fetch_timeout,
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

fn parse_token<R: io::BufRead>(elem: BytesStart, reader: &Reader<R>) -> Result<ParsedElement> {
    let role = match elem.try_get_attribute("role")? {
        Some(attr) => Some(attr.decode_and_unescape_value(reader)?.to_string()),
        None => None,
    };

    Ok(ParsedElement::Token(TokenAttributes { role }))
}

fn parse_word<R: io::BufRead>(elem: BytesStart, reader: &Reader<R>) -> Result<ParsedElement> {
    let role = match elem.try_get_attribute("role")? {
        Some(attr) => Some(attr.decode_and_unescape_value(reader)?.to_string()),
        None => None,
    };

    Ok(ParsedElement::Word(TokenAttributes { role }))
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
    let time = match elem.try_get_attribute("time")? {
        Some(time) => {
            let value = time.decode_and_unescape_value(reader)?;
            Some(TimeDesignation::from_str(&value)?)
        }
        None => None,
    };

    Ok(ParsedElement::Break(BreakAttributes { strength, time }))
}

fn parse_sub<R: io::BufRead>(elem: BytesStart, reader: &Reader<R>) -> Result<ParsedElement> {
    let alias = elem
        .try_get_attribute("alias")?
        .context("alias attribute required for sub element")?
        .decode_and_unescape_value(reader)?
        .to_string();

    Ok(ParsedElement::Sub(SubAttributes { alias }))
}

fn parse_language<R: io::BufRead>(elem: BytesStart, reader: &Reader<R>) -> Result<ParsedElement> {
    let lang = elem
        .try_get_attribute("xml:lang")?
        .context("xml:lang attribute is required with a lang element")?
        .decode_and_unescape_value(reader)?
        .to_string();

    let on_lang_failure = match elem.try_get_attribute("onlangfailure")? {
        Some(s) => {
            let value = s.decode_and_unescape_value(reader)?;
            Some(OnLanguageFailure::from_str(&value)?)
        }
        None => None,
    };

    Ok(ParsedElement::Lang(LangAttributes {
        lang,
        on_lang_failure,
    }))
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
    let duration = match elem.try_get_attribute("duration")? {
        Some(val) => Some(val.decode_and_unescape_value(reader)?.parse()?),
        None => None,
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

fn parse_mark<R: io::BufRead>(elem: BytesStart, reader: &Reader<R>) -> Result<ParsedElement> {
    let name = elem
        .try_get_attribute("name")?
        .context("name attribute is required with mark element")?
        .decode_and_unescape_value(reader)?
        .to_string();

    Ok(ParsedElement::Mark(MarkAttributes { name }))
}

fn parse_voice<R: io::BufRead>(elem: BytesStart, reader: &Reader<R>) -> Result<ParsedElement> {
    let gender = elem.try_get_attribute("gender")?;
    let gender = match gender {
        Some(v) => {
            let value = v.decode_and_unescape_value(reader)?;
            if value.is_empty() {
                None
            } else {
                Some(Gender::from_str(&value)?)
            }
        }
        None => None,
    };
    let age = elem.try_get_attribute("age")?;
    let age = match age {
        Some(v) => {
            let value = v.decode_and_unescape_value(reader)?;
            if value.is_empty() {
                None
            } else {
                Some(value.parse::<u8>()?)
            }
        }
        None => None,
    };
    let variant = elem.try_get_attribute("variant")?;
    let variant = match variant {
        Some(v) => {
            let value = v.decode_and_unescape_value(reader)?;
            if value.is_empty() {
                None
            } else {
                Some(value.parse::<NonZeroUsize>()?)
            }
        }
        None => None,
    };
    let name = elem.try_get_attribute("name")?;
    let name = match name {
        Some(v) => {
            let value = v.decode_and_unescape_value(reader)?;
            value
                .split(' ')
                .map(|x| x.to_string())
                .collect::<Vec<String>>()
        }
        None => vec![],
    };
    let languages = elem.try_get_attribute("languages")?;
    let languages = match languages {
        Some(v) => {
            let value = v.decode_and_unescape_value(reader)?;
            let mut res = vec![];
            for language in value.split(' ') {
                res.push(LanguageAccentPair::from_str(language)?);
            }
            res
        }
        None => vec![],
    };
    Ok(ParsedElement::Voice(VoiceAttributes {
        gender,
        age,
        variant,
        name,
        languages,
    }))
}

fn parse_audio<R: io::BufRead>(elem: BytesStart, reader: &Reader<R>) -> Result<ParsedElement> {
    let src = match elem.try_get_attribute("src")? {
        Some(s) => {
            let src: http::Uri = s.decode_and_unescape_value(reader)?.to_string().parse()?;
            Some(src)
        }
        None => None,
    };

    let fetch_timeout = match elem.try_get_attribute("fetchtimeout")? {
        Some(fetchtimeout) => {
            let fetchtimeout = fetchtimeout.decode_and_unescape_value(reader)?;
            Some(TimeDesignation::from_str(&fetchtimeout)?)
        }
        None => None,
    };

    let fetch_hint = match elem.try_get_attribute("fetchhint")? {
        Some(fetch) => {
            let fetch = fetch.decode_and_unescape_value(reader)?;
            FetchHint::from_str(&fetch)?
        }
        None => FetchHint::default(),
    };

    let max_age = if let Some(v) = elem.try_get_attribute("maxage")? {
        Some(v.decode_and_unescape_value(reader)?.parse::<usize>()?)
    } else {
        None
    };

    let max_stale = if let Some(v) = elem.try_get_attribute("maxage")? {
        Some(v.decode_and_unescape_value(reader)?.parse::<usize>()?)
    } else {
        None
    };

    let clip_begin = match elem.try_get_attribute("clipBegin")? {
        Some(clip) => {
            let clip = clip.decode_and_unescape_value(reader)?;
            TimeDesignation::from_str(&clip)?
        }
        None => TimeDesignation::Seconds(0.0),
    };

    let clip_end = match elem.try_get_attribute("clipBegin")? {
        Some(clip) => {
            let clip = clip.decode_and_unescape_value(reader)?;
            Some(TimeDesignation::from_str(&clip)?)
        }
        None => None,
    };

    let repeat_count = if let Some(v) = elem.try_get_attribute("repeatCount")? {
        v.decode_and_unescape_value(reader)?
            .parse::<NonZeroUsize>()?
    } else {
        unsafe { NonZeroUsize::new_unchecked(1) }
    };

    let repeat_dur = match elem.try_get_attribute("repeatDur")? {
        Some(repeat) => {
            let repeat = repeat.decode_and_unescape_value(reader)?;
            Some(TimeDesignation::from_str(&repeat)?)
        }
        None => None,
    };

    let sound_level = match elem.try_get_attribute("soundLevel")? {
        Some(sound) => {
            let sound = sound.decode_and_unescape_value(reader)?;
            parse_decibel(&sound)?
        }
        None => 0.0,
    };

    let speed = match elem.try_get_attribute("speed")? {
        Some(speed) => {
            let speed = speed.decode_and_unescape_value(reader)?;
            parse_unsigned_percentage(&speed)? / 100.0
        }
        None => 1.0,
    };

    Ok(ParsedElement::Audio(AudioAttributes {
        src,
        fetch_timeout,
        fetch_hint,
        max_age,
        max_stale,
        clip_begin,
        clip_end,
        repeat_count,
        repeat_dur,
        sound_level,
        speed,
    }))
}

pub(crate) fn parse_decibel(val: &str) -> anyhow::Result<f32> {
    lazy_static! {
        static ref DB_RE: Regex = Regex::new(r"^([+-]?(?:\d*\.)?\d+)dB$").unwrap();
    }
    let caps = DB_RE
        .captures(val)
        .context("value must be a valid decibel value")?;

    let num_val = caps[1].parse::<f32>()?;
    Ok(num_val)
}

/// returns percentages as written
pub(crate) fn parse_unsigned_percentage(val: &str) -> anyhow::Result<f32> {
    lazy_static! {
        static ref PERCENT_RE: Regex = Regex::new(r"^+?((?:\d*\.)?\d+)%$").unwrap();
    }
    let caps = PERCENT_RE
        .captures(val)
        .context("value must be a valid percentage value")?;

    let num_val = caps[1].parse::<f32>()?;
    Ok(num_val)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let unicode = parse_ssml(r#"<speak version="1.1">Let’s review a complex structure. Please note how threshold of control is calculated in this example.</speak>"#).unwrap();
        let ascii = parse_ssml(r#"<speak version="1.1">Let's review a complex structure. Please note how threshold of control is calculated in this example.</speak>"#).unwrap();

        let master_span_unicode = unicode.tags().next().unwrap();
        let master_span_ascii = ascii.tags().next().unwrap();

        assert_eq!(master_span_ascii.end, master_span_unicode.end);
        assert_eq!(master_span_ascii.end, ascii.get_text().chars().count());
    }

    #[test]
    fn span_contains() {
        let empty = parse_ssml(r#"<speak version="1.1"><break/><break/></speak>"#).unwrap();

        assert!(empty.tags[0].maybe_contains(&empty.tags[1]));
        assert!(empty.tags[0].maybe_contains(&empty.tags[2]));
        assert!(!empty.tags[1].maybe_contains(&empty.tags[2]));

        let hello =
            parse_ssml(r#"<speak version="1.1">Hello <s><w>hello</w></s> world <break/></speak>"#)
                .unwrap();
        assert!(hello.tags[0].maybe_contains(&hello.tags[1]));
        assert!(hello.tags[0].maybe_contains(&hello.tags[2]));
        assert!(hello.tags[0].maybe_contains(&hello.tags[3]));
        assert!(hello.tags[1].maybe_contains(&hello.tags[2]));
        assert!(!hello.tags[1].maybe_contains(&hello.tags[3]));
        assert!(!hello.tags[2].maybe_contains(&hello.tags[3]));

        let empty = parse_ssml(r#"<speak version="1.1">Hello <p></p><p></p></speak>"#).unwrap();
        assert!(!empty.tags[1].maybe_contains(&empty.tags[2]));

        let break_inside_custom = parse_ssml(r#"<speak version="1.1"><mstts:express-as style="string" styledegree="value" role="string">hello<break/> world</mstts:express-as></speak>"#).unwrap();
        assert!(break_inside_custom.tags[1].maybe_contains(&break_inside_custom.tags[2]));
    }

    #[test]
    fn reject_invalid_combos() {
        assert!(parse_ssml("<speak><speak>hello</speak></speak>").is_err());
        assert!(parse_ssml("<speak><p>hello<p>world</p></p></speak>").is_err());
    }

    #[test]
    fn skip_description_text() {
        let text = r#"<?xml version="1.0"?>
<speak xmlns="http://www.w3.org/2001/10/synthesis"
       xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
       xsi:schemaLocation="http://www.w3.org/2001/10/synthesis
                 http://www.w3.org/TR/speech-synthesis11/synthesis.xsd"
       xml:lang="en-US">
                 
  <!-- Normal use of <desc> -->
  Heads of State often make mistakes when speaking in a foreign language.
  One of the most well-known examples is that of John F. Kennedy:
  <audio src="ichbineinberliner.wav">If you could hear it, this would be
  a recording of John F. Kennedy speaking in Berlin.
    <desc>Kennedy's famous German language gaffe</desc>
  </audio>
</speak>"#;

        let res = parse_ssml(text).unwrap();

        assert_eq!(res.get_text().trim(),
                   "Heads of State often make mistakes when speaking in a foreign language. One of the most well-known examples is that of John F. Kennedy: If you could hear it, this would be a recording of John F. Kennedy speaking in Berlin.");
    }

    #[test]
    fn handle_language_elements() {
        let lang = r#"<speak version="1.1"><lang xml:lang="ja"></lang><lang xml:lang="en" onlangfailure="ignoretext"></lang></speak>"#;

        let res = parse_ssml(lang).unwrap();

        assert_eq!(res.tags.len(), 3);
        assert_eq!(
            res.tags[1].element,
            ParsedElement::Lang(LangAttributes {
                lang: "ja".to_string(),
                on_lang_failure: None
            })
        );
        assert_eq!(
            res.tags[2].element,
            ParsedElement::Lang(LangAttributes {
                lang: "en".to_string(),
                on_lang_failure: Some(OnLanguageFailure::IgnoreText)
            })
        );

        let lang = r#"<speak version="1.1"><lang lang="ja"></lang></speak>"#;

        assert!(parse_ssml(lang).is_err());
    }

    #[test]
    fn filter_out_elems() {
        let mut parser = SsmlParserBuilder::default().build().unwrap();

        assert!(parser.text_should_enter_buffer(Some(&SsmlElement::Sub)));
        assert!(!parser.text_should_enter_buffer(Some(&SsmlElement::Description)));

        parser.expand_sub = true;

        assert!(!parser.text_should_enter_buffer(Some(&SsmlElement::Sub)));
        assert!(!parser.text_should_enter_buffer(Some(&SsmlElement::Description)));
    }

    #[test]
    fn expand_sub() {
        let parser = SsmlParserBuilder::default()
            .expand_sub(true)
            .build()
            .unwrap();
        let sub =
            r#"<speak version="1.1"><sub alias="World wide web consortium">W3C</sub></speak>"#;

        let res = parser.parse(sub).unwrap();
        assert_eq!(res.get_text().trim(), "World wide web consortium");
        assert_eq!(res.event_log.len(), 3);
        assert!(matches!(res.event_log[1], ParserLogEvent::Text(_)));

        let parser = SsmlParserBuilder::default().build().unwrap();

        let res = parser.parse(sub).unwrap();
        assert_eq!(res.get_text().trim(), "W3C");

        assert_eq!(res.event_log.len(), 5);
    }

    #[test]
    fn decibels() {
        assert!(parse_decibel("56").is_err());
        assert!(parse_decibel("hello").is_err());
        assert!(parse_decibel("64.5DB").is_err());
        assert!(parse_decibel("64.5dBs").is_err());

        assert_eq!(parse_decibel("-10dB").unwrap() as i32, -10);
        assert_eq!(parse_decibel("15dB").unwrap() as i32, 15);
        assert_eq!(parse_decibel(".5dB").unwrap(), 0.5);
    }

    #[test]
    fn unsigned_percentages() {
        assert!(parse_unsigned_percentage("56").is_err());
        assert!(parse_unsigned_percentage("64pc").is_err());
        assert!(parse_unsigned_percentage("74%%").is_err());

        assert_eq!(parse_unsigned_percentage("10%").unwrap() as i32, 10);
        assert_eq!(parse_unsigned_percentage("110%").unwrap() as i32, 110);
        assert_eq!(parse_unsigned_percentage(".5%").unwrap(), 0.5);
    }
}
