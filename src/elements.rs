//! Documentation comments are taken in part from the SSML specification which
//! can be found [here](https://www.w3.org/TR/speech-synthesis11). All copied
//! sections will be marked with:
//!
//! "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
//! All Rights Reserved._
//!
//! If any sections aren't marked please submit a PR. For types this copyright
//! notice will be placed on the top level type and not each field for conciseness
//! but keep in mind the fields will also be taken from the same section of the
//! standard.
use anyhow::{bail, Context};
use lazy_static::lazy_static;
use quick_xml::escape::escape;
use regex::Regex;
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::fmt::{self, Display};
use std::num::NonZeroUsize;
use std::str::FromStr;
use std::time::Duration;

/// Type of the SSML element
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum SsmlElement {
    /// The `<speak></speak>` element.
    Speak,
    /// The `<lexicon/>` element.
    Lexicon,
    /// The `<lookup></lookup>` element.
    Lookup,
    /// The `<meta/>` element.
    Meta,
    /// The `<metadata></metadata>` element.
    Metadata,
    /// The `<p></p>` element.
    Paragraph,
    /// The `<s></s>` element.
    Sentence,
    /// The `<token></token>` element.
    Token,
    /// The `<word></word>` element.
    Word,
    /// The `<say-as></say-as>` element.
    SayAs,
    /// The `<phoneme></phoneme>` element.
    Phoneme,
    /// The `<sub></sub>` element.
    Sub,
    /// The `<lang></lang>` element.
    Lang,
    /// The `<voice></voice>` element.
    Voice,
    /// The `<emphasis></emphasis>` element.
    Emphasis,
    /// The `<break/>` element.
    Break,
    /// The `<prosody></prosody>` element.
    Prosody,
    /// The `<audio></audio>` element.
    Audio,
    /// The `<mark/>` element.
    Mark,
    /// The `<desc></desc>` element.
    Description,
    /// Custom elements not defined in the spec, the element name is stored in the given string.
    Custom(String),
}

impl SsmlElement {
    /// Returns whether a tag can contain other tags - will always be true for custom tags as we
    /// want to check just in case.
    #[inline(always)]
    pub fn can_contain_tags(&self) -> bool {
        // empty elements
        // * Lexicon
        // * Meta
        // * Metadata (can contain content but is ignored by synthesis processor)
        // * say-as can only contain text to render (word is the same)
        // * phoneme is text only
        // * sub subtitles  only (no elements)
        // * description is only for inside audio tag and not to be rendered
        // * mark element is empty element used as a bookmark
        matches!(
            self,
            Self::Speak
                | Self::Paragraph
                | Self::Sentence
                | Self::Voice
                | Self::Emphasis
                | Self::Token
                | Self::Word
                | Self::Lang
                | Self::Prosody
                | Self::Audio
                | Self::Custom(_)
        )
    }

    /// Check whether the provided element can contain another specified tag. For custom elements
    /// if an element can contain tags it will be assumed it can contain the custom one as these
    /// are outside of the SSML specification.
    pub fn can_contain(&self, other: &Self) -> bool {
        match (self, other) {
            (a, Self::Custom(_)) if a.can_contain_tags() => true,
            (a, _) if !a.can_contain_tags() => false,
            (_, Self::Speak) => false,
            (Self::Speak, _) => true,
            (Self::Paragraph, a) => a.allowed_in_paragraph(),
            (Self::Sentence, a) => a.allowed_in_sentence(),
            (Self::Voice, a) => a.allowed_in_speak(), // Everything allowed inside
            (Self::Emphasis, a) => a.allowed_in_sentence(), // Emphasis and sentence lists match
            (Self::Token | Self::Word, a) => a.allowed_in_token(),
            (Self::Lang, a) => a.allowed_in_speak(),
            (Self::Prosody, a) => a.allowed_in_speak(),
            (Self::Audio, a) => a.allowed_in_speak(),
            (Self::Custom(_), _) => true,
            _ => false, // Should be unreachable
        }
    }

    /// Returns true if an SSML element is allowed within a paragraph `<p>...</p>`
    #[inline(always)]
    fn allowed_in_paragraph(&self) -> bool {
        matches!(self, Self::Sentence) || self.allowed_in_sentence()
    }

    /// Returns true if an SSML element is allowed within a sentence `<s>...</s>`
    #[inline(always)]
    fn allowed_in_sentence(&self) -> bool {
        matches!(
            self,
            Self::Custom(_)
                | Self::Audio
                | Self::Break
                | Self::Emphasis
                | Self::Lang
                | Self::Lookup
                | Self::Mark
                | Self::Phoneme
                | Self::Prosody
                | Self::SayAs
                | Self::Sub
                | Self::Token
                | Self::Voice
                | Self::Word
        )
    }

    /// Returns true if an SSML element is allowed within `<speak></speak>`
    #[inline(always)]
    fn allowed_in_speak(&self) -> bool {
        self != &Self::Speak
    }

    #[inline(always)]
    fn allowed_in_token(&self) -> bool {
        matches!(
            self,
            Self::Audio
                | Self::Break
                | Self::Emphasis
                | Self::Mark
                | Self::Phoneme
                | Self::Prosody
                | Self::SayAs
                | Self::Sub
                | Self::Custom(_)
        )
    }

    /// Returns true if the text inside should be processed by the speech synthesiser. Returns
    /// false for custom elements.
    #[inline(always)]
    pub(crate) fn contains_synthesisable_text(&self) -> bool {
        !matches!(
            self,
            Self::Description
                | Self::Metadata
                | Self::Mark
                | Self::Break
                | Self::Lexicon
                | Self::Meta
        )
    }
}

impl Display for SsmlElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use SsmlElement::*;
        write!(
            f,
            "{}",
            match self {
                Custom(name) => name,
                Speak => "speak",
                Lexicon => "lexicon",
                Lookup => "lookup",
                Meta => "meta",
                Metadata => "metadata",
                Paragraph => "p",
                Sentence => "s",
                Token => "token",
                Word => "w",
                SayAs => "say-as",
                Phoneme => "phoneme",
                Sub => "sub",
                Lang => "lang",
                Voice => "voice",
                Emphasis => "emphasis",
                Break => "break",
                Prosody => "prosody",
                Audio => "audio",
                Mark => "mark",
                Description => "desc",
            }
        )
    }
}

/// Enum representing the parsed element, each element with attributes allowed also contains an
/// object for it's attributes.
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedElement {
    /// The `<speak></speak>` element and given attributes.
    Speak(SpeakAttributes),
    /// The `<lexicon/>` element and given attributes.
    // TODO: spec mentions `lexicon` can only be immediate children of `speak`. enforce this check
    Lexicon(LexiconAttributes),
    /// The `<lookup></lookup>` element and given attributes.
    Lookup(LookupAttributes),
    /// The `<meta/> element and given attributes.
    Meta(MetaAttributes),
    /// The `<metadata></metadata>` element.
    Metadata,
    /// The `<p></p>` element.
    Paragraph,
    /// The `<s></s>` element.
    Sentence,
    /// The `<token></token>` element and given attributes.
    Token(TokenAttributes),
    /// The `<word></word>` element and given attributes.
    // `w` element is just an alias for `token`
    Word(TokenAttributes),
    /// The `<say-as></say-as>` element and given attributes.
    SayAs(SayAsAttributes),
    /// The `<phoneme></phoneme>` element and given attributes.
    Phoneme(PhonemeAttributes),
    /// The `<sub></sub>` element and given attributes.
    Sub(SubAttributes),
    /// The `<lang></lang>` element and given attributes.
    Lang(LangAttributes),
    /// The `<voice></voice>` element and given attributes.
    Voice(VoiceAttributes),
    /// The `<emphasis></emphasis>` element and given attributes.
    Emphasis(EmphasisAttributes),
    /// The `<break/>` element and given attributes.
    Break(BreakAttributes),
    /// The `<prosody></prosody>` element and given attributes.
    Prosody(ProsodyAttributes),
    /// The `<audio></audio>` element and given attributes.
    Audio(AudioAttributes),
    /// The `<mark/>` element and given attributes.
    Mark(MarkAttributes),
    /// The `<desc></desc>` element and given attributes.
    Description(String),
    /// Custom elements not defined in the spec, the element name is stored in the given string and
    /// any attributes in the map.
    Custom((String, BTreeMap<String, String>)),
}

impl ParsedElement {
    /// From an element get the XML attribute string - this is used for writing the SSML back out
    pub fn attribute_string(&self) -> String {
        use ParsedElement::*;

        match self {
            Lexicon(attr) => format!("{}", attr),
            Lookup(attr) => format!("{}", attr),
            Meta(attr) => format!("{}", attr),
            Metadata => String::new(),
            Paragraph => String::new(),
            Sentence => String::new(),
            Token(attr) => format!("{}", attr),
            Word(attr) => format!("{}", attr),
            SayAs(attr) => format!("{}", attr),
            Speak(attr) => format!("{}", attr),
            Phoneme(attr) => format!("{}", attr),
            Sub(attr) => format!("{}", attr),
            Lang(attr) => format!("{}", attr),
            Voice(attr) => format!("{}", attr),
            Emphasis(attr) => format!("{}", attr),
            Break(attr) => format!("{}", attr),
            Prosody(attr) => format!("{}", attr),
            Audio(attr) => format!("{}", attr),
            Mark(attr) => format!("{}", attr),
            Description(_) => String::new(),
            Custom((_, attr_map)) => {
                let mut attr_str = String::new();
                for (name, val) in attr_map.iter() {
                    attr_str.push_str(&format!(" {}=\"{}\"", name, val));
                }
                attr_str
            }
        }
    }

    /// Returns true if an SSML element can contain tags
    pub fn can_contain_tags(&self) -> bool {
        SsmlElement::from(self).can_contain_tags()
    }

    /// Returns true if an SSML element can contain another element
    pub fn can_contain(&self, other: &Self) -> bool {
        SsmlElement::from(self).can_contain(&SsmlElement::from(other))
    }
}

impl From<&ParsedElement> for SsmlElement {
    fn from(elem: &ParsedElement) -> Self {
        match elem {
            ParsedElement::Speak(_) => Self::Speak,
            ParsedElement::Lexicon(_) => Self::Lexicon,
            ParsedElement::Lookup(_) => Self::Lookup,
            ParsedElement::Meta(_) => Self::Meta,
            ParsedElement::Metadata => Self::Metadata,
            ParsedElement::Paragraph => Self::Paragraph,
            ParsedElement::Sentence => Self::Sentence,
            ParsedElement::Token(_) => Self::Token,
            ParsedElement::Word(_) => Self::Word,
            ParsedElement::SayAs(_) => Self::SayAs,
            ParsedElement::Phoneme(_) => Self::Phoneme,
            ParsedElement::Sub(_) => Self::Sub,
            ParsedElement::Lang(_) => Self::Lang,
            ParsedElement::Voice(_) => Self::Voice,
            ParsedElement::Emphasis(_) => Self::Emphasis,
            ParsedElement::Break(_) => Self::Break,
            ParsedElement::Prosody(_) => Self::Prosody,
            ParsedElement::Audio(_) => Self::Audio,
            ParsedElement::Mark(_) => Self::Mark,
            ParsedElement::Description(_) => Self::Description,
            ParsedElement::Custom((s, _)) => Self::Custom(s.to_string()),
        }
    }
}

/// The Speech Synthesis Markup Language is an XML application. The root element is speak.
///
/// N.B. According to the standard version is a required attribute, however we haven't found any
/// TTS providers that enforce that rule so implement a laxer parsing for compatibility with the
/// wider ecosystem.
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SpeakAttributes {
    /// Lang is an attribute specifying the language of the root document. In the specification
    /// this is a REQUIRED attribute, however in reality most TTS APIs require a different way to
    /// specify the language outside of SSML and treat this as optional. Because of that this
    /// implementation has chosen to be more permissive than the spec.
    pub lang: Option<String>,
    /// Base is an OPTIONAL attribute specifying the Base URI of the root document.
    pub base: Option<String>,
    /// On Language Failure is an OPTIONAL attribute specifying the desired behavior upon language speaking failure.
    pub on_lang_failure: Option<OnLanguageFailure>,
    /// The version attribute is a REQUIRED attribute that indicates the version of the specification to be used for the document and MUST have the value "1.1".
    pub version: String,
    /// for remaining attributes on root like namespace etc
    pub xml_root_attrs: BTreeMap<String, String>,
}

#[cfg(test)]
impl fake::Dummy<fake::Faker> for SpeakAttributes {
    fn dummy_with_rng<R: rand::Rng + ?Sized>(f: &fake::Faker, rng: &mut R) -> Self {
        use fake::Fake;
        Self {
            lang: f.fake_with_rng(rng),
            base: f.fake_with_rng(rng),
            on_lang_failure: f.fake_with_rng(rng),
            version: "1.1".to_string(),
            xml_root_attrs: f.fake_with_rng(rng),
        }
    }
}

impl Display for SpeakAttributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, " version=\"{}\"", escape(&self.version))?;
        if let Some(lang) = &self.lang {
            write!(f, " xml:lang=\"{}\"", escape(lang))?;
        }
        if let Some(base) = &self.base {
            write!(f, " xml:base=\"{}\"", escape(base))?;
        }
        if let Some(fail) = &self.on_lang_failure {
            write!(f, " onlangfailure=\"{}\"", fail)?;
        }
        for (attr_name, attr_value) in self.xml_root_attrs.iter() {
            write!(f, " {}=\"{}\"", attr_name, attr_value)?;
        }
        Ok(())
    }
}

/// The lang element is used to specify the natural language of the content. This element MAY be used when there is a change in the natural language.
#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct LangAttributes {
    /// Lang is a REQUIRED attribute specifying the language of the root document.
    pub lang: String,
    /// On Language Failure is an OPTIONAL attribute specifying the desired behavior upon language speaking failure.
    pub on_lang_failure: Option<OnLanguageFailure>,
}

impl Display for LangAttributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, " xml:lang=\"{}\"", escape(&self.lang))?;
        if let Some(fail) = self.on_lang_failure {
            write!(f, " onlangfailure=\"{}\"", fail)?;
        }

        Ok(())
    }
}

/// The onlangfailure attribute is an optional attribute that contains one value
/// from the following enumerated list describing the desired behavior of the
/// synthesis processor upon language speaking failure. A conforming synthesis
/// processor must report a language speaking failure in addition to taking th
/// action(s) below.
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum OnLanguageFailure {
    /// If a voice exists that can speak the language, the synthesis processor
    /// will switch to that voice and speak the content. Otherwise, the
    /// processor chooses another behavior (either ignoretext or ignorelang).
    ChangeVoice,
    /// The synthesis processor will not attempt to render the text that is in
    /// the failed language.
    IgnoreText,
    /// The synthesis processor will ignore the change in language and speak as
    /// if the content were in the previous language.
    IgnoreLang,
    /// The synthesis processor chooses the behavior (either changevoice, ignoretext,
    /// or ignorelang).
    ProcessorChoice,
}

impl Display for OnLanguageFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use OnLanguageFailure::*;
        write!(
            f,
            "{}",
            match self {
                ChangeVoice => "changevoice",
                IgnoreText => "ignoretext",
                IgnoreLang => "ignorelang",
                ProcessorChoice => "processorchoice",
            }
        )
    }
}

impl FromStr for OnLanguageFailure {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = match s {
            "changevoice" => Self::ChangeVoice,
            "ignoretext" => Self::IgnoreText,
            "ignorelang" => Self::IgnoreLang,
            "processorchoice" => Self::ProcessorChoice,
            e => bail!("Unrecognised language failure value {}", e),
        };
        Ok(s)
    }
}

impl FromStr for SsmlElement {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = match s {
            "speak" => Self::Speak,
            "lexicon" => Self::Lexicon,
            "lookup" => Self::Lookup,
            "meta" => Self::Meta,
            "metadata" => Self::Metadata,
            "p" => Self::Paragraph,
            "s" => Self::Sentence,
            "token" => Self::Token,
            "w" => Self::Word,
            "say-as" => Self::SayAs,
            "phoneme" => Self::Phoneme,
            "sub" => Self::Sub,
            "lang" => Self::Lang,
            "voice" => Self::Voice,
            "emphasis" => Self::Emphasis,
            "break" => Self::Break,
            "prosody" => Self::Prosody,
            "audio" => Self::Audio,
            "mark" => Self::Mark,
            "desc" => Self::Description,
            e => Self::Custom(e.to_string()),
        };
        Ok(s)
    }
}

/// An SSML document MAY reference one or more lexicon documents. A lexicon
/// document is located by a URI with an OPTIONAL media type and is assigned a
/// name that is unique in the SSML document. Any number of lexicon elements MAY
/// occur as immediate children of the speak element.
///
/// The lexicon element MUST have a uri attribute specifying a URI that identifies
/// the location of the lexicon document.
///
/// The lexicon element MUST have an xml:id attribute that assigns a name to the
/// lexicon document. The name MUST be unique to the current SSML document. The
/// scope of this name is the current SSML document.
///
/// The lexicon element MAY have a type attribute that specifies the media type of
/// the lexicon document. The default value of the type attribute is
/// application/pls+xml, the media type associated with Pronunciation Lexicon
/// Specification [PLS] documents as defined in [RFC4267].
///
/// The lexicon element MAY have a fetchtimeout attribute that specifies the timeout
/// for fetches. The value is a Time Designation. The default value is processor-specific.
///
/// The lexicon element MAY have a maxage attribute that indicates that the document is
/// willing to use content whose age is no greater than the specified time
/// (cf. 'max-age' in HTTP 1.1 [RFC2616]). The value is an xsd:nonNegativeInteger
/// [SCHEMA2 §3.3.20]. The document is not willing to use stale content, unless maxstale
/// is also provided.
///
/// The lexicon element MAY have a maxstale attribute that indicates that the document is
/// willing to use content that has exceeded its expiration time (cf. 'max-stale' in HTTP 1.1
/// [RFC2616]). The value is an xsd:nonNegativeInteger [SCHEMA2 §3.3.20]. If maxstale is
/// assigned a value, then the document is willing to accept content that has exceeded its
/// expiration time by no more than the specified amount of time.
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Debug, Clone, PartialEq)]
pub struct LexiconAttributes {
    ///  The lexicon element MUST have a uri attribute specifying a URI that identifies the location of the lexicon document.
    pub uri: http::Uri,
    /// The lexicon element MUST have an xml:id attribute that assigns a name to the lexicon document. The name MUST be unique to the current SSML document.
    /// The scope of this name is the current SSML document.
    pub xml_id: String,
    /// The lexicon element MAY have a type attribute that specifies the media type of the lexicon
    /// document. The default value of the type attribute is application/pls+xml, the media type
    /// associated with Pronunciation Lexicon Specification documents.
    pub ty: Option<mediatype::MediaTypeBuf>,
    /// The lexicon element MAY have a fetchtimeout attribute that specifies the timeout for fetches.
    pub fetch_timeout: Option<TimeDesignation>,
    // TODO we don't support maxage or maxstale
}

#[cfg(test)]
impl fake::Dummy<fake::Faker> for LexiconAttributes {
    fn dummy_with_rng<R: rand::Rng + ?Sized>(f: &fake::Faker, rng: &mut R) -> Self {
        use fake::Fake;
        use mediatype::names::*;
        let ty = if rng.gen_bool(0.5) {
            Some(mediatype::MediaTypeBuf::new(
                APPLICATION,
                mediatype::Name::new("pls+xml").unwrap(),
            ))
        } else {
            None
        };
        Self {
            uri: f.fake_with_rng(rng),
            xml_id: f.fake_with_rng(rng),
            fetch_timeout: f.fake_with_rng(rng),
            ty,
        }
    }
}

impl Display for LexiconAttributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, " uri=\"{}\"", escape(&self.uri.to_string()))?;
        write!(f, " xml:id=\"{}\"", escape(&self.xml_id))?;
        if let Some(ty) = &self.ty {
            write!(f, " type=\"{}\"", ty)?;
        }
        if let Some(timeout) = &self.fetch_timeout {
            write!(f, " fetchtimeout=\"{}\"", timeout)?;
        }
        Ok(())
    }
}

/// For times SSML only uses seconds or milliseconds in the form "%fs" "%fs", this handles parsing
/// these times
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum TimeDesignation {
    /// Time specified in seconds
    Seconds(f32),
    /// Time specified in milliseconds
    Milliseconds(f32),
}

impl TimeDesignation {
    /// Turns the time designation to a std Duration type.
    pub fn duration(&self) -> Duration {
        match self {
            Self::Seconds(s) => Duration::from_secs_f32(*s),
            Self::Milliseconds(ms) => Duration::from_secs_f32(ms / 1000.0),
        }
    }
}

impl Display for TimeDesignation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Seconds(val) => format!("{}s", val),
                Self::Milliseconds(val) => format!("{}ms", val),
            }
        )
    }
}

impl FromStr for TimeDesignation {
    type Err = anyhow::Error;

    fn from_str(time: &str) -> Result<Self, Self::Err> {
        lazy_static! {
            static ref TIME_RE: Regex = Regex::new(r"^\+?((?:\d*\.)?\d+)\s*(s|ms)$").unwrap();
        }
        let caps = TIME_RE
            .captures(time)
            .context("attribute must be a valid TimeDesignation")?;

        let num_val = caps[1].parse::<f32>().unwrap();

        match &caps[2] {
            "s" => Ok(TimeDesignation::Seconds(num_val)),
            "ms" => Ok(TimeDesignation::Milliseconds(num_val)),
            _ => unreachable!(),
        }
    }
}

/// The lookup element MUST have a ref attribute. The ref attribute specifies a
/// name that references a lexicon document as assigned by the xml:id attribute
/// of the lexicon element.
///
/// The referenced lexicon document may contain information (e.g., pronunciation)
///  for tokens that can appear in a text to be rendered. For PLS lexicon documents
/// , the information contained within the PLS document MUST be used by the synthesis
///  processor when rendering tokens that appear within the context of a lookup
/// element. For non-PLS lexicon documents, the information contained within the
/// lexicon document SHOULD be used by the synthesis processor when rendering tokens
/// that appear within the content of a lookup element, although the processor MAY
/// choose not to use the information if it is deemed incompatible with the content
/// of the SSML document. For example, a vendor-specific lexicon may be used only for
/// particular values of the interpret-as attribute of the say-as element, or for a
/// particular set of voices. Vendors SHOULD document the expected behavior of the
/// synthesis processor when SSML content refers to a non-PLS lexicon.
///
/// A lookup element MAY contain other lookup elements. When a lookup element contains
/// other lookup elements, the child lookup elements have higher precedence. Precedence
/// means that a token is first looked up in the lexicon with highest precedence. Only
/// if the token is not found in that lexicon is it then looked up in the lexicon with
/// the next lower precedence, and so on until the token is successfully found or until
/// all lexicons have been used for lookup. It is assumed that the synthesis processor
/// already has one or more built-in system lexicons which will be treated as having
/// a lower precedence than those specified using the lexicon and lookup elements.
/// Note that if a token is not within the scope of at least one lookup element, then
/// the token can only be looked up in the built-in system lexicons.
///
/// The lookup element can only contain text to be rendered and the following elements:
/// audio, break, emphasis, lang, lookup, mark, p, phoneme, prosody, say-as, sub, s,
/// token, voice, w.
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct LookupAttributes {
    /// Specifies a name that references a lexicon document as assigned by the xml:id attribute of the lexicon element.
    pub lookup_ref: String,
}

impl Display for LookupAttributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, " ref=\"{}\"", escape(&self.lookup_ref))
    }
}

/// The metadata and meta elements are containers in which information about the
/// document can be placed. The metadata element provides more general and powerful
/// treatment of metadata information than meta by using a metadata schema.
///
/// A meta declaration associates a string to a declared meta property or declares
/// "http-equiv" content. Either a name or http-equiv attribute is REQUIRED. It is
/// an error to provide both name and http-equiv attributes. A content attribute is
/// REQUIRED. The seeAlso property is the only defined meta property name. It is
/// used to specify a resource that might provide additional metadata information
/// about the content. This property is modeled on the seeAlso property of Resource
/// Description Framework (RDF) Schema Specification 1.0 [RDF-SCHEMA §5.4.1]. The
/// http-equiv attribute has a special significance when documents are retrieved
/// via HTTP. Although the preferred method of providing HTTP header information is
/// by using HTTP header fields, the "http-equiv" content MAY be used in situations
/// where the SSML document author is unable to configure HTTP header fields
/// associated with their document on the origin server, for example, cache control
/// information. Note that HTTP servers and caches are not required to introspect
/// the contents of meta in SSML documents and thereby override the header values
/// they would send otherwise.
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaAttributes {
    /// Currently, the only defined name is `seeAlso`. In future other meta names may be added.
    pub name: Option<String>,
    /// Used for when documents are retrieved via HTTP.
    pub http_equiv: Option<String>,
    /// The content referred to by the meta.
    pub content: String,
}

#[cfg(test)]
impl fake::Dummy<fake::Faker> for MetaAttributes {
    fn dummy_with_rng<R: rand::Rng + ?Sized>(f: &fake::Faker, rng: &mut R) -> Self {
        use fake::Fake;
        let (name, http_equiv) = if rng.gen_bool(0.5) {
            (None, Some(f.fake_with_rng(rng)))
        } else {
            (Some(f.fake_with_rng(rng)), None)
        };
        Self {
            name,
            http_equiv,
            content: f.fake_with_rng(rng),
        }
    }
}

impl Display for MetaAttributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, " content=\"{}\"", escape(&self.content))?;
        if let Some(http_equiv) = &self.http_equiv {
            write!(f, " http-equiv=\"{}\"", escape(http_equiv))?;
        }
        if let Some(name) = &self.name {
            write!(f, " name=\"{}\"", escape(name))?;
        }

        Ok(())
    }
}
/// The token element allows the author to indicate its content is a token and to
/// eliminate token (word) segmentation ambiguities of the synthesis processor.
///
/// The token element is necessary in order to render languages
///  - that do not use white space as a token boundary identifier, such as Chinese,
///    Thai, and Japanese
///  - that use white space for syllable segmentation, such as Vietnamese
///  - that use white space for other purposes, such as Urdu
///
/// Use of this element can result in improved cues for prosodic control (e.g.,
/// pause) and may assist the synthesis processor in selection of the correct
/// pronunciation for homographs. Other elements such as break, mark, and prosody
/// are permitted within token to allow annotation at a sub-token level (e.g.,
/// syllable, mora, or whatever units are reasonable for the current language).
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct TokenAttributes {
    /// `role` is an OPTIONAL defined attribute on the token element. The role
    /// attribute takes as its value one or more white space separated QNames
    /// (as defined in Section 4 of Namespaces in XML (1.0 [XMLNS 1.0] or 1.1
    /// [XMLNS 1.1], depending on the version of XML being used)). A QName in
    /// the attribute content is expanded into an expanded-name using the
    /// namespace declarations in scope for the containing token element. Thus,
    ///  each QName provides a reference to a specific item in the designated
    /// namespace. In the second example below, the QName within the role
    /// attribute expands to the "VV0" item in the
    /// "http://www.example.com/claws7tags" namespace. This mechanism allows
    /// for referencing defined taxonomies of word classes, with the expectation
    /// that they are documented at the specified namespace URI.
    ///
    /// The role attribute is intended to be of use in synchronizing with other
    /// specifications, for example to describe additional information to help
    /// the selection of the most appropriate pronunciation for the contained
    /// text inside an external lexicon (see lexicon documents).
    pub role: Option<String>,
}

impl Display for TokenAttributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(role) = &self.role {
            write!(f, " role=\"{}\"", escape(role))?;
        }

        Ok(())
    }
}

/// The say-as element allows the author to indicate information on the type of text
/// construct contained within the element and to help specify the level of detail
/// for rendering the contained text.
/// The say-as element has three attributes: interpret-as, format, and detail.
/// The interpret-as attribute is always required; the other two attributes are optional.
/// The legal values for the format attribute depend on the value of the interpret-as attribute.
/// The say-as element can only contain text to be rendered.
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct SayAsAttributes {
    /// The interpret-as attribute indicates the content type of the contained text construct.
    /// Specifying the content type helps the synthesis processor to distinguish and interpret
    /// text constructs that may be rendered in different ways depending on what type of
    /// information is intended.
    pub interpret_as: String,
    /// The optional format attribute can give further hints on the precise formatting of the
    /// contained text for content types that may have ambiguous formats.
    pub format: Option<String>,
    /// The detail attribute is an optional attribute that indicates the level of detail to be
    /// read aloud or rendered. Every value of the detail attribute must render all of the
    /// informational content in the contained text; however, specific values for the detail
    /// attribute can be used to render content that is not usually informational in running
    /// text but may be important to render for specific purposes. For example, a synthesis
    /// processor will usually render punctuations through appropriate changes in prosody.
    /// Setting a higher level of detail may be used to speak punctuations explicitly,
    /// e.g. for reading out coded part numbers or pieces of software code.
    pub detail: Option<String>,
}

impl Display for SayAsAttributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, " interpret-as=\"{}\"", escape(&self.interpret_as))?;
        if let Some(format) = &self.format {
            write!(f, " format=\"{}\"", escape(format))?;
        }
        if let Some(detail) = &self.detail {
            write!(f, " detail=\"{}\"", escape(detail))?
        }

        Ok(())
    }
}

/// The phonemic/phonetic pronunciation alphabet. A pronunciation alphabet in this context refers to a collection
/// of symbols to represent the sounds of one or more human languages.
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum PhonemeAlphabet {
    /// The Internation Phonetic Association's alphabet.
    Ipa,
    /// Another alphabet (only IPA is required to be supported).
    Other(String),
}

impl Display for PhonemeAlphabet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Ipa => "ipa".into(),
                Self::Other(alphabet) => escape(alphabet),
            }
        )
    }
}

impl FromStr for PhonemeAlphabet {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ipa" => Ok(Self::Ipa),
            e => Ok(Self::Other(e.to_string())),
        }
    }
}

/// The phoneme element provides a phonemic/phonetic pronunciation for the
/// contained text. The phoneme element may be empty. However, it is recommended
/// that the element contain human-readable text that can be used for non-spoken
/// rendering of the document. For example, the content may be displayed visually
/// for users with hearing impairments.
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct PhonemeAttributes {
    /// The ph attribute is a required attribute that specifies the phoneme/phone
    /// string.
    pub ph: String,
    /// The alphabet attribute is an optional attribute that specifies the
    /// phonemic/phonetic pronunciation alphabet. A pronunciation alphabet
    /// in this context refers to a collection of symbols to represent the
    /// sounds of one or more human languages. The only valid values for this
    /// attribute are "ipa", values defined in the
    /// [Pronunciation Alphabet Registry](https://www.w3.org/TR/speech-synthesis11/#S3.1.10.1)
    /// and vendor-defined strings of the form "x-organization" or
    /// "x-organization-alphabet". For example, the Japan Electronics and
    /// Information Technology Industries Association (JEITA) might wish to
    /// encourage the use of an alphabet such as "x-JEITA" or "x-JEITA-IT-4002"
    /// for their phoneme alphabet (JEIDAALPHABET).
    pub alphabet: Option<PhonemeAlphabet>,
}

impl Display for PhonemeAttributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, " ph=\"{}\"", escape(&self.ph))?;
        if let Some(alphabet) = &self.alphabet {
            write!(f, " alphabet=\"{}\"", alphabet)?;
        }

        Ok(())
    }
}

///  The strength attribute is an optional attribute having one of the following
///  values: "none", "x-weak", "weak", "medium" (default value), "strong", or
///  "x-strong". This attribute is used to indicate the strength of the prosodic
///  break in the speech output. The value "none" indicates that no prosodic
///  break boundary should be outputted, which can be used to prevent a prosodic
///  break which the processor would otherwise produce. The other values
///  indicate monotonically non-decreasing (conceptually increasing) break
///  strength between tokens. The stronger boundaries are typically accompanied
///  by pauses. "x-weak" and "x-strong" are mnemonics for "extra weak" and
///  "extra strong", respectively.
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum Strength {
    /// None value - do not insert a break here
    No,
    /// Extra weak break (x-weak)
    ExtraWeak,
    /// Weak break
    Weak,
    /// Medium break (default)
    Medium,
    /// Strong break
    Strong,
    /// Extra strong break (x-strong)
    ExtraStrong,
}

impl Display for Strength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::No => "none",
                Self::ExtraWeak => "x-weak",
                Self::Weak => "weak",
                Self::Medium => "medium",
                Self::Strong => "strong",
                Self::ExtraStrong => "x-strong",
            }
        )
    }
}

impl FromStr for Strength {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_ref() {
            "none" => Ok(Self::No),
            "x-weak" => Ok(Self::ExtraWeak),
            "weak" => Ok(Self::Weak),
            "medium" => Ok(Self::Medium),
            "strong" => Ok(Self::Strong),
            "x-strong" => Ok(Self::ExtraStrong),
            e => bail!("Unrecognised strength value {}", e),
        }
    }
}

/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum PitchStrength {
    /// Extra low (x-low)
    XLow,
    /// Low
    Low,
    /// Medium
    Medium,
    /// High
    High,
    /// Extra high (x-high)
    XHigh,
    /// Default
    Default,
}

impl FromStr for PitchStrength {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "x-low" => Ok(Self::XLow),
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "x-high" => Ok(Self::XHigh),
            "default" => Ok(Self::Default),
            e => bail!("Unrecognised value {}", e),
        }
    }
}

impl Display for PitchStrength {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let pitch_strength = match self {
            PitchStrength::XLow => "x-low",
            PitchStrength::Low => "low",
            PitchStrength::Medium => "medium",
            PitchStrength::High => "high",
            PitchStrength::XHigh => "x-high",
            PitchStrength::Default => "default",
        };
        write!(fmt, "{}", pitch_strength)
    }
}

/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum VolumeStrength {
    /// Silent
    Silent,
    /// X-soft
    XSoft,
    /// Soft
    Soft,
    /// Medium
    Medium,
    /// Loud
    Loud,
    /// X-loud
    XLoud,
    /// Default
    Default,
}

impl FromStr for VolumeStrength {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "silent" => Ok(Self::Silent),
            "x-soft" => Ok(Self::XSoft),
            "soft" => Ok(Self::Soft),
            "medium" => Ok(Self::Medium),
            "loud" => Ok(Self::Loud),
            "x-loud" => Ok(Self::XLoud),
            "default" => Ok(Self::Default),
            e => bail!("Unrecognised value {}", e),
        }
    }
}

impl fmt::Display for VolumeStrength {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let volume_strength = match self {
            VolumeStrength::Silent => "silent",
            VolumeStrength::XSoft => "x-soft",
            VolumeStrength::Soft => "soft",
            VolumeStrength::Medium => "medium",
            VolumeStrength::Loud => "loud",
            VolumeStrength::XLoud => "x-loud",
            VolumeStrength::Default => "default",
        };
        write!(fmt, "{}", volume_strength)
    }
}

/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum RateStrength {
    /// X-slow
    XSlow,
    /// Slow
    Slow,
    /// Medium
    Medium,
    /// Fast
    Fast,
    /// X-fast
    XFast,
    /// Default
    Default,
}

impl FromStr for RateStrength {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "x-slow" => Ok(Self::XSlow),
            "slow" => Ok(Self::Slow),
            "medium" => Ok(Self::Medium),
            "fast" => Ok(Self::Fast),
            "x-fast" => Ok(Self::XFast),
            "default" => Ok(Self::Default),
            e => bail!("Unrecognised value {}", e),
        }
    }
}

impl fmt::Display for RateStrength {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let rate_strength = match self {
            RateStrength::XSlow => "x-slow",
            RateStrength::Slow => "slow",
            RateStrength::Medium => "medium",
            RateStrength::Fast => "fast",
            RateStrength::XFast => "x-fast",
            RateStrength::Default => "default",
        };
        write!(fmt, "{}", rate_strength)
    }
}

/// Sign for relative values (positive or negative).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum Sign {
    /// Positive relative change.
    Plus,
    /// Negative relative change.
    Minus,
}

impl fmt::Display for Sign {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Plus => write!(fmt, "+"),
            Self::Minus => write!(fmt, "-"),
        }
    }
}

/// Although the exact meaning of "pitch range" will vary across synthesis processors,
/// increasing/decreasing this value will typically increase/decrease the dynamic range of the output pitch.
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum PitchRange {
    /// Specifies the range in terms of a strength enum
    Strength(PitchStrength), // low, medium high etc
    /// Specify it in terms of absolute frequencies
    Frequency(f32),
    /// Specifies the range in terms of relative changes between an existing pitch.
    RelativeChange((f32, Sign, Unit)),
}

impl FromStr for PitchRange {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "x-low" => Ok(Self::Strength(PitchStrength::XLow)),
            "low" => Ok(Self::Strength(PitchStrength::Low)),
            "medium" => Ok(Self::Strength(PitchStrength::Medium)),
            "high" => Ok(Self::Strength(PitchStrength::High)),
            "x-high" => Ok(Self::Strength(PitchStrength::XHigh)),
            "default" => Ok(Self::Strength(PitchStrength::Default)),
            value if value.ends_with("Hz") || value.ends_with('%') || value.ends_with("st") => {
                if value.ends_with("Hz") {
                    if value.starts_with('+') || value.starts_with('-') {
                        if value.starts_with('-') {
                            Ok(Self::RelativeChange((
                                value.strip_suffix("Hz").unwrap().parse::<f32>()? * -1.0,
                                Sign::Minus,
                                Unit::Hz,
                            )))
                        } else {
                            Ok(Self::RelativeChange((
                                value.strip_suffix("Hz").unwrap().parse::<f32>()?,
                                Sign::Plus,
                                Unit::Hz,
                            )))
                        }
                    } else {
                        Ok(Self::Frequency(
                            value.strip_suffix("Hz").unwrap().parse::<f32>()?,
                        ))
                    }
                } else if value.ends_with('%') {
                    if value.starts_with('+') || value.starts_with('-') {
                        if value.starts_with('-') {
                            Ok(Self::RelativeChange((
                                value.strip_suffix('%').unwrap().parse::<f32>()? * -1.0,
                                Sign::Minus,
                                Unit::Percentage,
                            )))
                        } else {
                            Ok(Self::RelativeChange((
                                value.strip_suffix('%').unwrap().parse::<f32>()?,
                                Sign::Plus,
                                Unit::Percentage,
                            )))
                        }
                    } else {
                        bail!("Unrecognised value {}", value);
                    }
                } else if value.ends_with("st") {
                    if value.starts_with('+') || value.starts_with('-') {
                        if value.starts_with('-') {
                            Ok(Self::RelativeChange((
                                value.strip_suffix("st").unwrap().parse::<f32>()? * -1.0,
                                Sign::Minus,
                                Unit::St,
                            )))
                        } else {
                            Ok(Self::RelativeChange((
                                value.strip_suffix("st").unwrap().parse::<f32>()?,
                                Sign::Plus,
                                Unit::St,
                            )))
                        }
                    } else {
                        bail!("Unrecognised value {}", value);
                    }
                } else {
                    bail!("Unrecognised value {}", value);
                }
            }
            e => bail!("Unrecognised value {}", e),
        }
    }
}

impl fmt::Display for PitchRange {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Strength(strength) => write!(fmt, "{}", strength),
            Self::Frequency(frequency) => write!(fmt, "{}Hz", frequency),
            Self::RelativeChange((relchange, sign, unit)) => {
                write!(fmt, "{}{}{}", sign, relchange, unit)
            }
        }
    }
}

/// The volume for the contained text. Legal values are: a number preceded by "+" or "-" and
/// immediately followed by "dB"; or "silent", "x-soft", "soft", "medium", "loud", "x-loud", or
/// "default". The default is +0.0dB. Specifying a value of "silent" amounts to specifying minus infinity
/// decibels (dB).
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum VolumeRange {
    /// Specifies the volume via an enumeration
    Strength(VolumeStrength), // "silent", "x-soft", "soft", "medium", "loud", "x-loud", default
    /// Volume specified via Decibels
    Decibel(f32),
}

impl FromStr for VolumeRange {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "silent" => Ok(Self::Strength(VolumeStrength::Silent)),
            "x-soft" => Ok(Self::Strength(VolumeStrength::XSoft)),
            "soft" => Ok(Self::Strength(VolumeStrength::Soft)),
            "medium" => Ok(Self::Strength(VolumeStrength::Medium)),
            "loud" => Ok(Self::Strength(VolumeStrength::Loud)),
            "x-loud" => Ok(Self::Strength(VolumeStrength::XLoud)),
            "default" => Ok(Self::Strength(VolumeStrength::Default)),
            value if value.ends_with("dB") => Ok(Self::Decibel(
                value.strip_suffix("dB").unwrap().parse::<f32>()?,
            )),
            e => bail!("Unrecognised value {}", e),
        }
    }
}

impl fmt::Display for VolumeRange {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Strength(strength) => write!(fmt, "{}", strength),
            Self::Decibel(percent) => write!(fmt, "{}dB", percent),
        }
    }
}

///  A change in the speaking rate for the contained text. Legal values are: a non-negative percentage or "x-slow",
///  "slow", "medium", "fast", "x-fast", or "default". Labels "x-slow" through "x-fast" represent a sequence of
///  monotonically non-decreasing speaking rates. When the value is a non-negative percentage it acts as a multiplier
///  of the default rate. For example, a value of 100% means no change in speaking rate, a value of 200% means a
///  speaking rate twice the default rate, and a value of 50% means a speaking rate of half the default rate.
///  The default rate for a voice depends on the language and dialect and on the personality of the voice.
///  The default rate for a voice SHOULD be such that it is experienced as a normal speaking rate for the voice when
///  reading aloud text. Since voices are processor-specific, the default rate will be as well.
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum RateRange {
    /// Rate rate specified via an enum.
    Strength(RateStrength), // "x-slow", "slow", "medium", "fast", "x-fast", or "default"
    /// Rate range specified via a positive percentage.
    Percentage(PositiveNumber),
}

impl FromStr for RateRange {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "x-slow" => Ok(Self::Strength(RateStrength::XSlow)),
            "slow" => Ok(Self::Strength(RateStrength::Slow)),
            "medium" => Ok(Self::Strength(RateStrength::Medium)),
            "fast" => Ok(Self::Strength(RateStrength::Fast)),
            "x-fast" => Ok(Self::Strength(RateStrength::XFast)),
            "default" => Ok(Self::Strength(RateStrength::Default)),
            value if value.ends_with('%') => {
                if value.starts_with('+') || value.starts_with('-') {
                    if value.starts_with('+') {
                        Ok(Self::Percentage(
                            value.strip_suffix('%').unwrap().parse::<PositiveNumber>()?,
                        ))
                    } else {
                        bail!(
                            "Unrecognised value {}",
                            "Negative percentage not allowed for rate"
                        );
                    }
                } else {
                    Ok(Self::Percentage(
                        value.strip_suffix('%').unwrap().parse::<PositiveNumber>()?,
                    ))
                }
            }
            e => bail!("Unrecognised value {}", e),
        }
    }
}

impl fmt::Display for RateRange {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Strength(strength) => write!(fmt, "{}", strength),
            Self::Percentage(percent) => write!(fmt, "{}%", percent),
        }
    }
}

/// The pitch contour is defined as a set of white space-separated targets at specified time positions in the speech output.
/// The algorithm for interpolating between the targets is processor-specific. In each pair of the form (time position,target),
/// the first value is a percentage of the period of the contained text (a number followed by "%") and the second value is
/// the value of the pitch attribute (a number followed by "Hz", a relative change, or a label value). Time position values
/// outside 0% to 100% are ignored. If a pitch value is not defined for 0% or 100% then the nearest pitch target is copied.
/// All relative values for the pitch are relative to the pitch value just before the contained text.
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum ContourElement {
    /// Pitch contouring element.
    Element((f32, PitchRange)),
}

impl FromStr for ContourElement {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            value if value.starts_with('(') && value.ends_with(')') => {
                let value = value.strip_suffix(')').unwrap().to_string();
                let value = value.strip_prefix('(').unwrap().to_string();
                let elements = value.split(',').collect::<Vec<_>>();

                let pitch = match PitchRange::from_str(elements[1]) {
                    Ok(result) => result,
                    Err(e) => bail!("Error: {}", e),
                };

                if elements[0].ends_with('%') {
                    let percentage = elements[0].strip_suffix('%').unwrap().parse::<f32>()?;
                    Ok(Self::Element((percentage, pitch)))
                } else {
                    bail!(
                        "Unrecognised value {}",
                        "Invalid percentage in pitch contour"
                    );
                }
            }
            e => bail!("Unrecognised value {}", e),
        }
    }
}

impl fmt::Display for ContourElement {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Element((pct, pitch_range)) => {
                write!(fmt, "({}%,{})", pct, pitch_range)
            }
        }
    }
}

/// The pitch contour is defined as a set of white space-separated targets at specified time positions in the speech output.
/// The algorithm for interpolating between the targets is processor-specific. In each pair of the form (time position,target),
/// the first value is a percentage of the period of the contained text (a number followed by "%") and the second value is
/// the value of the pitch attribute (a number followed by "Hz", a relative change, or a label value). Time position values
/// outside 0% to 100% are ignored. If a pitch value is not defined for 0% or 100% then the nearest pitch target is copied.
/// All relative values for the pitch are relative to the pitch value just before the contained text.
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Clone, Debug, PartialEq, PartialOrd)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum PitchContour {
    /// List of pitch contours
    Elements(Vec<ContourElement>),
}

impl FromStr for PitchContour {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut pitch_contour_elements = Vec::new();
        match s {
            value if value.starts_with('(') => {
                let elements = value.split(' ').collect::<Vec<_>>();

                for element in elements {
                    let pitchcontourelement = ContourElement::from_str(element)?;
                    pitch_contour_elements.push(pitchcontourelement);
                }

                Ok(Self::Elements(pitch_contour_elements))
            }
            e if !e.trim().is_empty() => bail!("Unrecognised value {}", e),
            _ => Ok(Self::Elements(pitch_contour_elements)), // No op on pitch contouring
        }
    }
}

impl fmt::Display for PitchContour {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut all_elements_str = "".to_string();
        let mut start = true;
        match self {
            Self::Elements(elements) => {
                for element in elements {
                    let element_str = element.to_string();

                    if !start {
                        all_elements_str.push(' ');
                    }
                    all_elements_str.push_str(&element_str);

                    if start {
                        start = false;
                    }
                }
                write!(fmt, "{}", all_elements_str)
            }
        }
    }
}

/// Representation of positive numbers in SSML tags. We keep a float vs integral value to ensure
/// that when re-serializating numeric errors are minimised.
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub enum PositiveNumber {
    /// Floating point value
    FloatNumber(f32),
    /// Integral number
    RoundNumber(isize),
}

#[cfg(test)]
impl fake::Dummy<fake::Faker> for PositiveNumber {
    fn dummy_with_rng<R: rand::Rng + ?Sized>(_: &fake::Faker, rng: &mut R) -> PositiveNumber {
        if rng.gen_bool(0.5) {
            Self::FloatNumber(rng.gen_range(0.1..100.0))
        } else {
            Self::RoundNumber(rng.gen_range(1..100))
        }
    }
}

impl FromStr for PositiveNumber {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            value
                if value.starts_with('+')
                    || value.starts_with('-')
                    || value.parse::<f32>().is_ok() =>
            {
                if value.starts_with('+') {
                    if value.contains('.') {
                        Ok(Self::FloatNumber(
                            value.strip_prefix('+').unwrap().parse::<f32>()?,
                        ))
                    } else {
                        Ok(Self::RoundNumber(
                            value.strip_prefix('+').unwrap().parse::<isize>()?,
                        ))
                    }
                } else if value.starts_with('-') {
                    bail!("Unrecognised value {}", "Negative number not allowed");
                } else if value.contains('.') {
                    Ok(Self::FloatNumber(value.parse::<f32>()?))
                } else {
                    Ok(Self::RoundNumber(value.parse::<isize>()?))
                }
            }
            e => bail!("Unrecognised value {}", e),
        }
    }
}

impl fmt::Display for PositiveNumber {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::FloatNumber(floatnum) => write!(fmt, "{}", floatnum),
            Self::RoundNumber(roundnum) => write!(fmt, "{}", roundnum),
        }
    }
}

/// Unit used to measure relative changes in values, this is either percentage or for pitches can
/// be measured in semitones or Hertz.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum Unit {
    /// Hertz
    Hz,
    /// Semi-tone
    St,
    /// Percentage
    Percentage,
}

impl fmt::Display for Unit {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Hz => write!(fmt, "Hz"),
            Self::St => write!(fmt, "st"),
            Self::Percentage => write!(fmt, "%"),
        }
    }
}

/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum EmphasisLevel {
    /// Strong
    Strong,
    /// Moderate (default)
    Moderate,
    /// None
    None,
    /// Reduced
    Reduced,
}

impl Display for EmphasisLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Strong => "strong",
                Self::Moderate => "moderate",
                Self::None => "none",
                Self::Reduced => "reduced",
            }
        )
    }
}

impl FromStr for EmphasisLevel {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "strong" => Ok(Self::Strong),
            "moderate" => Ok(Self::Moderate),
            "none" => Ok(Self::None),
            "reduced" => Ok(Self::Reduced),
            e => bail!("Unrecognised value {}", e),
        }
    }
}

/// The break element is an empty element that controls the pausing or other
/// prosodic boundaries between tokens. The use of the break element between
/// any pair of tokens is optional. If the element is not present between
/// tokens, the synthesis processor is expected to automatically determine a
/// break based on the linguistic context. In practice, the break element is
/// most often used to override the typical automatic behavior of a synthesis
/// processor.
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Clone, Debug, PartialEq, PartialOrd)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct BreakAttributes {
    ///  The strength attribute is an optional attribute having one of the following
    ///  values: "none", "x-weak", "weak", "medium" (default value), "strong", or
    ///  "x-strong". This attribute is used to indicate the strength of the prosodic
    ///  break in the speech output. The value "none" indicates that no prosodic
    ///  break boundary should be outputted, which can be used to prevent a prosodic
    ///  break which the processor would otherwise produce. The other values
    ///  indicate monotonically non-decreasing (conceptually increasing) break
    ///  strength between tokens. The stronger boundaries are typically accompanied
    ///  by pauses. "x-weak" and "x-strong" are mnemonics for "extra weak" and
    ///  "extra strong", respectively.
    pub strength: Option<Strength>,
    /// The time attribute is an optional attribute indicating the duration of a
    /// pause to be inserted in the output in seconds or milliseconds. It
    /// follows the time value format from the Cascading Style Sheets Level 2
    /// Recommendation [CSS2], e.g. "250ms",
    pub time: Option<TimeDesignation>,
}

impl Display for BreakAttributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(strength) = self.strength {
            write!(f, " strength=\"{}\"", strength)?;
        }
        if let Some(time) = &self.time {
            write!(f, " time=\"{}\"", time)?;
        }
        Ok(())
    }
}

/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Clone, Debug, PartialEq, PartialOrd)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct ProsodyAttributes {
    /// pitch: the baseline pitch for the contained text. Although the exact meaning of "baseline pitch"
    /// will vary across synthesis processors, increasing/decreasing this value will typically increase/decrease
    /// the approximate pitch of the output. Legal values are: a number followed by "Hz", a relative change
    /// or "x-low", "low", "medium", "high", "x-high", or "default". Labels "x-low" through "x-high" represent
    /// a sequence of monotonically non-decreasing pitch levels.
    pub pitch: Option<PitchRange>,
    /// The pitch contour is defined as a set of white space-separated targets at specified
    /// time positions in the speech output. The algorithm for interpolating between the targets
    /// is processor-specific. In each pair of the form (time position,target), the first value
    /// is a percentage of the period of the contained text (a number followed by "%") and
    /// the second value is the value of the pitch attribute (a number followed by "Hz", a relative
    /// change, or a label value). Time position values outside 0% to 100% are ignored.
    /// If a pitch value is not defined for 0% or 100% then the nearest pitch target is copied.
    /// All relative values for the pitch are relative to the pitch value just before the contained text.
    pub contour: Option<PitchContour>,
    /// the pitch range (variability) for the contained text. Although the exact meaning of
    /// "pitch range" will vary across synthesis processors, increasing/decreasing this value
    /// will typically increase/decrease the dynamic range of the output pitch. Legal values
    /// are: a number followed by "Hz", a relative change or "x-low", "low", "medium", "high",
    /// "x-high", or "default". Labels "x-low" through "x-high" represent a sequence of
    /// monotonically non-decreasing pitch ranges.
    pub range: Option<PitchRange>,
    /// a change in the speaking rate for the contained text. Legal values are: a non-negative
    /// percentage or "x-slow", "slow", "medium", "fast", "x-fast", or "default". Labels "x-slow"
    /// through "x-fast" represent a sequence of monotonically non-decreasing speaking rates.
    /// When the value is a non-negative percentage it acts as a multiplier of the default rate.
    /// For example, a value of 100% means no change in speaking rate, a value of 200% means a
    /// speaking rate twice the default rate, and a value of 50% means a speaking rate of half
    /// the default rate. The default rate for a voice depends on the language and dialect and on
    /// the personality of the voice. The default rate for a voice should be such that it is
    /// experienced as a normal speaking rate for the voice when reading aloud text. Since voices
    /// are processor-specific, the default rate will be as well.
    pub rate: Option<RateRange>,
    /// duration: a value in seconds or milliseconds for the desired time to take to read the
    /// contained text. Follows the time value format from the Cascading Style Sheet Level 2
    /// Recommendation [CSS2], e.g. "250ms", "3s".
    pub duration: Option<TimeDesignation>,
    /// the volume for the contained text. Legal values are: a number preceded by "+" or "-"
    /// and immediately followed by "dB"; or "silent", "x-soft", "soft", "medium", "loud", "x-loud",
    /// or "default". The default is +0.0dB. Specifying a value of "silent" amounts to specifying
    /// minus infinity decibels (dB). Labels "silent" through "x-loud" represent a sequence of
    /// monotonically non-decreasing volume levels. When the value is a signed number (dB),
    /// it specifies the ratio of the squares of the new signal amplitude (a1) and the current
    /// amplitude (a0), and is defined in terms of dB:
    pub volume: Option<VolumeRange>,
}

impl Display for ProsodyAttributes {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        if let Some(pitch) = &self.pitch {
            write!(f, " pitch=\"{}\"", pitch)?;
        }
        if let Some(contour) = &self.contour {
            write!(f, " contour=\"{}\"", contour)?;
        }
        if let Some(range) = &self.range {
            write!(f, " range=\"{}\"", range)?;
        }
        if let Some(rate) = &self.rate {
            write!(f, " rate=\"{}\"", rate)?;
        }
        if let Some(duration) = &self.duration {
            write!(f, " duration=\"{}\"", duration)?;
        }
        if let Some(volume) = &self.volume {
            write!(f, " volume=\"{}\"", volume)?;
        }
        Ok(())
    }
}

/// A mark element is an empty element that places a marker into the text/tag
/// sequence. It has one REQUIRED attribute, name, which is of type xsd:token
/// [SCHEMA2 §3.3.2]. The mark element can be used to reference a specific
/// location in the text/tag sequence, and can additionally be used to insert a
/// marker into an output stream for asynchronous notification. When processing
/// a mark element, a synthesis processor MUST do one or both of the following:
///  - inform the hosting environment with the value of the name attribute and
///  with information allowing the platform to retrieve the corresponding position
///  in the rendered output.
///  - when audio output of the SSML document reaches the mark, issue an event that
///  includes the REQUIRED name attribute of the element. The hosting environment
///  defines the destination of the event.
///
/// The mark element does not affect the speech output process.
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct MarkAttributes {
    /// Name of the marker used to refer to it when jumping in the audio.
    pub name: String,
}

impl Display for MarkAttributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, " name=\"{}\"", escape(&self.name))
    }
}

/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct EmphasisAttributes {
    /// the optional level attribute indicates the strength of emphasis to be applied. Defined
    /// values are "strong", "moderate", "none" and "reduced". The default level is "moderate".
    /// The meaning of "strong" and "moderate" emphasis is interpreted according to the language
    /// being spoken (languages indicate emphasis using a possible combination of pitch change,
    /// timing changes, loudness and other acoustic differences). The "reduced" level is effectively
    /// the opposite of emphasizing a word. For example, when the phrase "going to" is reduced it
    /// may be spoken as "gonna". The "none" level is used to prevent the synthesis processor from
    /// emphasizing words that it might typically emphasize. The values "none", "moderate", and "strong"
    /// are monotonically non-decreasing in strength.
    pub level: Option<EmphasisLevel>,
}

impl Display for EmphasisAttributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(level) = self.level {
            write!(f, " level=\"{}\"", level)?;
        }

        Ok(())
    }
}

/// The sub element is employed to indicate that the text in the alias attribute
/// value replaces the contained text for pronunciation. This allows a document to
/// contain both a spoken and written form. The REQUIRED alias attribute specifies
/// the string to be spoken instead of the enclosed string. The processor SHOULD
/// apply text normalization to the alias value.
///
/// The sub element can only contain text (no elements).
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct SubAttributes {
    /// The string to be spoken instead of the string enclosed in the tag
    pub alias: String,
}

impl Display for SubAttributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, " alias=\"{}\"", escape(&self.alias))
    }
}

/// Attribute indicating the preferred gender of the voice to speak the contained text.
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum Gender {
    /// Male voice
    Male,
    /// Female voice
    Female,
    /// Gender neutral voice
    Neutral,
}

impl Display for Gender {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Male => "male",
                Self::Female => "female",
                Self::Neutral => "neutral",
            }
        )
    }
}

impl FromStr for Gender {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "male" => Ok(Self::Male),
            "female" => Ok(Self::Female),
            "neutral" => Ok(Self::Neutral),
            e => bail!("Unrecognised gender value {}", e),
        }
    }
}

/// A language accent pair, this will be a language (required) and an optional accent in which to
/// speak the language.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct LanguageAccentPair {
    /// Language the voice is desired to speak.
    pub lang: String,
    /// Optional accent to apply to the language.
    pub accent: Option<String>,
}

impl Display for LanguageAccentPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", escape(&self.lang))?;
        if let Some(accent) = &self.accent {
            write!(f, ":{}", escape(accent))?;
        }
        Ok(())
    }
}

impl FromStr for LanguageAccentPair {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            bail!("Empty language string");
        } else if s == "und" || s == "zxx" {
            bail!("Disallowed language code");
        } else {
            let lang_accent = s.split(':').collect::<Vec<_>>();
            if lang_accent.len() > 2 {
                bail!(
                    "Invalid format 'language:accent' or 'language' expected for '{}'",
                    s
                );
            }
            if lang_accent.len() == 1 {
                Ok(LanguageAccentPair {
                    lang: lang_accent[0].to_string(),
                    accent: None,
                })
            } else if lang_accent.len() == 2 {
                Ok(LanguageAccentPair {
                    lang: lang_accent[0].to_string(),
                    accent: Some(lang_accent[1].to_string()),
                })
            } else {
                bail!("Unexpected language accent pair: '{}'", s);
            }
        }
    }
}

/// The voice element is a production element that requests a change in speaking voice. There are
/// two kinds of attributes for the voice element: those that indicate desired features of a
/// voice and those that control behavior. The voice feature attributes are:
///
/// * **gender**: _optional_ attribute indicating the preferred gender of the voice to speak the
/// contained text. Enumerated values are: "male", "female", "neutral", or the empty string "".
/// * **age**: _optional_ attribute indicating the preferred age in years (since birth) of the
/// voice to speak the contained text. Acceptable values are of type xsd:nonNegativeInteger
/// [SCHEMA2 §3.3.20] or the empty string "".
/// * **variant**: _optional_ attribute indicating a preferred variant of the other voice
/// characteristics to speak the contained text. (e.g. the second male child voice). Valid values of
/// variant are of type xsd:positiveInteger [SCHEMA2 §3.3.25] or the empty string "".
/// * **name**: _optional_ attribute indicating a processor-specific voice name to speak the contained
/// text. The value may be a space-separated list of names ordered from top preference down or the
/// empty string "". As a result a name must not contain any white space.
/// * **languages**: _optional_ attribute indicating the list of languages the voice is desired to speak.
/// The value must be either the empty string "" or a space-separated list of languages, with optional
/// accent indication per language. Each language/accent pair is of the form "language" or
/// "language:accent", where both language and accent must be an Extended Language Range
/// [BCP47, Matching of Language Tags §2.2], except that the values "und" and "zxx" are disallowed.
/// A voice satisfies the languages feature if, for each language/accent pair in the list,
///   1. the voice is documented (see Voice descriptions) as reading/speaking a language that
///   matches the Extended Language Range given by language according to the Extended Filtering
///   matching algorithm [BCP47, Matching of Language Tags §3.3.2], and
///   2. if an accent is given, the voice is documented (see Voice descriptions) as
///   reading/speaking the language above with an accent that matches the Extended Language Range
///   given by accent according to the Extended Filtering matching algorithm [BCP47, Matching of
///   Language Tags §3.3.2], except that the script and extension subtags of the accent must be
///   ignored by the synthesis processor. It is recommended that authors and voice providers do
///   not use the script or extension subtags for accents because they are not relevant for
///   speaking.
///
/// For example, a languages value of "en:pt fr:ja" can legally be matched by any voice that can
/// both read English (speaking it with a Portuguese accent) and read French (speaking it with a
/// Japanese accent). Thus, a voice that only supports "en-US" with a "pt-BR" accent and "fr-CA"
/// with a "ja" accent would match. As another example, if we have <voice languages="fr:pt"> and
/// there is no voice that supports French with a Portuguese accent, then a voice selection
/// failure will occur. Note that if no accent indication is given for a language, then any voice
/// that speaks the language is acceptable, regardless of accent. Also, note that author control
/// over language support during voice selection is independent of any value of xml:lang in the
/// text.
///
/// For the feature attributes above, an empty string value indicates that any voice will satisfy
/// the feature. The top-level default value for all feature attributes is "", the empty string.
///
/// The behavior control attributes of voice are:
///
/// * **required**: _optional_ attribute that specifies a set of features by their respective
/// attribute names. This set of features is used by the voice selection algorithm described below.
/// Valid values of required are a space-separated list composed of values from the list of feature
/// names: "name", "languages", "gender", "age", "variant" or the empty string "". The default
/// value for this attribute is "languages".
/// * **ordering**: _optional_ attribute that specifies the priority ordering of features. Valid
/// values of ordering are a space-separated list composed of values from the list of feature
/// names: "name", "languages", "gender", "age", "variant" or the empty string "", where features
/// named earlier in the list have higher priority . The default value for this attribute is
/// "languages". Features not listed in the ordering list have equal priority to each other but
/// lower than that of the last feature in the list. Note that if the ordering attribute is set to
/// the empty string then all features have the same priority.
/// * **onvoicefailure**: _optional_ attribute containing one value from the following enumerated
/// list describing the desired behavior of the synthesis processor upon voice selection failure.
/// The default value for this attribute is "priorityselect".
///     * *priorityselect* - the synthesis processor uses the values of all voice feature attributes
///     to select a voice by feature priority, where the starting candidate set is the set of all
///     available voices.
///     * *keepexisting* - the voice does not change.
///     * *processorchoice* - the synthesis processor chooses the behavior (either priorityselect or
///     keepexisting).
///
/// The following voice selection algorithm must be used:
///
/// 1. All available voices are identified for which the values of all voice feature attributes
/// listed in the required attribute value are matched. When the value of the required attribute is
/// the empty string "", any and all voices are considered successful matches. If one or more voices
/// are identified, the selection is considered successful; otherwise there is voice selection
/// failure.
/// 2. If a successful selection identifies only one voice, the synthesis processor must use that
/// voice.
/// 3. If a successful selection identifies more than one voice, the remaining features (those not
/// listed in the required attribute value) are used to choose a voice by feature priority, where
/// the starting candidate set is the set of all voices identified.
/// 4. If there is voice selection failure, a conforming synthesis processor must report the voice
/// selection failure in addition to taking the action(s) expressed by the value of the
/// onvoicefailure attribute.
/// 5. To choose a voice by feature priority, each feature is taken in turn starting with the
/// highest priority feature, as controlled by the ordering attribute.
///     * If at least one voice matches the value of the current voice feature attribute then all
///     voices not matching that value are removed from the candidate set. If a single voice remains
///     in the candidate set the synthesis processor must use it. If more than one voice remains in
///     the candidate set then the next priority feature is examined for the candidate set.
///     * If no voices match the value of the current voice feature attribute then the next priority
///     feature is examined for the candidate set.
/// 6. After examining all feature attributes on the ordering list, if multiple voices remain in
/// the candidate set, the synthesis processor must use any one of them.
///
/// Although each attribute individually is optional, it is an error if no attributes are specified
/// when the voice element is used.
///
/// # Voice descriptions
/// For every voice made available to a synthesis processor, the vendor of the voice must document the
/// following:
///
/// * a list of language tags [BCP47, Tags for Identifying Languages] representing the languages the
/// voice can read.
/// * for each language, a language tag [BCP47, Tags for Identifying Languages] representing the
/// accent the voice uses when reading the language.
///
/// Although indication of language (using xml:lang) and selection of voice (using voice) are
/// independent, there is no requirement that a synthesis processor support every possible
/// combination of values of the two. However, a synthesis processor must document expected
/// rendering behavior for every possible combination. See the onlangfailure attribute for
/// information on what happens when the processor encounters text content that the voice cannot
/// speak.
///
/// voice attributes are inherited down the tree including to within elements that change the
/// language. The defaults described for each attribute only apply at the top (document) level and
/// are overridden by explicit author use of the voice element. In addition, changes in voice are
/// scoped and apply only to the content of the element in which the change occurred. When
/// processing reaches the end of a voice element content, i.e. the closing </voice> tag, the voice
/// in effect before the beginning tag is restored.
///
/// Similarly, if a voice is changed by the processor as a result of a language speaking failure,
/// the prior voice is restored when that voice is again able to speak the content. Note that there
/// is always an active voice, since the synthesis processor is required to select a default voice
/// before beginning execution of the document.
///
/// Relative changes in prosodic parameters should be carried across voice changes. However,
/// different voices have different natural defaults for pitch, speaking rate, etc. because they
/// represent different personalities, so absolute values of the prosodic parameters may vary across
/// changes in the voice.
///
/// The quality of the output audio or voice may suffer if a change in voice is requested within a
/// sentence.
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct VoiceAttributes {
    /// OPTIONAL attribute indicating the preferred gender of the voice to speak the contained text.
    /// Enumerated values are: "male", "female", "neutral", or the empty string "".
    pub gender: Option<Gender>,
    /// OPTIONAL attribute indicating the preferred age in years (since birth) of the voice to speak the contained text.
    pub age: Option<u8>,
    /// OPTIONAL attribute indicating a preferred variant of the other voice characteristics to speak the contained text.
    /// (e.g. the second male child voice).
    pub variant: Option<NonZeroUsize>,
    ///  OPTIONAL attribute indicating a processor-specific voice name to speak the contained text.
    ///  The value MAY be a space-separated list of names ordered from top preference down or the empty string "".
    ///  As a result a name MUST NOT contain any white space.
    pub name: Vec<String>,
    /// OPTIONAL attribute indicating the list of languages the voice is desired to speak.
    /// The value MUST be either the empty string "" or a space-separated list of languages,
    /// with OPTIONAL accent indication per language. Each language/accent pair is of the form "language" or "language:accent",
    /// where both language and accent MUST be an Extended Language Range, except that the values "und" and "zxx" are disallowed.
    pub languages: Vec<LanguageAccentPair>,
}

impl Display for VoiceAttributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(gender) = self.gender {
            write!(f, " gender=\"{}\"", gender)?;
        }
        if let Some(age) = self.age {
            write!(f, " age=\"{}\"", age)?;
        }
        if let Some(variant) = self.variant {
            write!(f, " variant=\"{}\"", variant)?;
        }
        if !self.name.is_empty() {
            write!(f, " name=\"{}\"", escape(&self.name.join(" ")))?;
        }
        if !self.languages.is_empty() {
            let languages_str = self
                .languages
                .iter()
                .map(|l| format!("{}", l))
                .collect::<Vec<String>>()
                .join(" ");

            write!(f, " languages=\"{}\"", languages_str)?;
        }

        Ok(())
    }
}

/// This tells the synthesis processor whether or not it can attempt to optimize rendering by pre-fetching audio.
/// The value is either safe to say that audio is only fetched when it is needed, never before; or prefetch to permit,
/// but not require the processor to pre-fetch the audio.
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum FetchHint {
    /// The processor can perform an optimisation where it fetches the audio before it is needed
    Prefetch,
    /// The audio should only be fetched when needed
    Safe,
}

impl Display for FetchHint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Prefetch => "prefetch",
                Self::Safe => "safe",
            }
        )
    }
}

impl FromStr for FetchHint {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = match s {
            "prefetch" => Self::Prefetch,
            "safe" => Self::Safe,
            e => bail!("Unrecognised fetchhint {}", e),
        };
        Ok(s)
    }
}

impl Default for FetchHint {
    fn default() -> Self {
        Self::Prefetch
    }
}

/// The audio element supports the insertion of recorded audio files and the insertion of other
/// audio formats in conjunction with synthesized speech output. The audio element may be empty.
/// If the audio element is not empty then the contents should be the marked-up text to be spoken if the audio document is not available. The alternate content may include text, speech markup, desc elements, or other audio elements. The alternate content may also be used when rendering the document to non-audible output and for accessibility (see the desc element).
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct AudioAttributes {
    /// The URI of a document with an appropriate media type. If absent, the audio element behaves
    /// as if src were present with a legal URI but the document could not be fetched.
    pub src: Option<http::Uri>,
    /// The timeout for fetches.
    pub fetch_timeout: Option<TimeDesignation>,
    /// This tells the synthesis processor whether or not it can attempt to optimize rendering by
    /// pre-fetching audio. The value is either safe to say that audio is only fetched when it is
    /// needed, never before; or prefetch to permit, but not require the processor to pre-fetch the
    /// audio.
    pub fetch_hint: FetchHint,
    /// Indicates that the document is willing to use content whose age is no greater than the
    /// specified time (cf. 'max-age' in HTTP 1.1). The document is not willing to use
    /// stale content, unless maxstale is also provided.
    pub max_age: Option<usize>,
    /// Indicates that the document is willing to use content that has exceeded its expiration time
    /// (cf. 'max-stale' in HTTP 1.1). If maxstale is assigned a value, then the document is willing
    /// to accept content that has exceeded its expiration time by no more than the specified amount
    /// of time.
    pub max_stale: Option<usize>,
    // Trimming attributes
    /// offset from start of media to begin rendering. This offset is measured in normal media
    /// playback time from the beginning of the media.
    pub clip_begin: TimeDesignation,
    /// offset from start of media to end rendering. This offset is measured in normal media
    /// playback time from the beginning of the media.
    pub clip_end: Option<TimeDesignation>,
    /// number of iterations of media to render. A fractional value describes a portion of the
    /// rendered media.
    pub repeat_count: NonZeroUsize,
    /// total duration for repeatedly rendering media. This duration is measured in normal media
    /// playback time from the beginning of the media.
    pub repeat_dur: Option<TimeDesignation>,
    /// Sound level in decibels
    pub sound_level: f32,
    /// Speed in a percentage where 1.0 corresponds to 100%
    pub speed: f32,
}

impl Display for AudioAttributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, " fetchhint=\"{}\"", self.fetch_hint)?;
        write!(f, " clipBegin=\"{}\"", self.clip_begin)?;
        write!(f, " repeatCount=\"{}\"", self.repeat_count)?;
        write!(f, " soundLevel=\"{}dB\"", self.sound_level)?;
        write!(f, " speed=\"{}%\"", self.speed * 100.0)?;
        if let Some(src) = &self.src {
            write!(f, " src=\"{}\"", escape(&src.to_string()))?;
        }
        if let Some(timeout) = &self.fetch_timeout {
            write!(f, " fetchtimeout=\"{}\"", timeout)?;
        }
        if let Some(max_age) = &self.max_age {
            write!(f, " maxage=\"{}\"", max_age)?;
        }
        if let Some(max_stale) = &self.max_stale {
            write!(f, " maxstale=\"{}\"", max_stale)?;
        }
        if let Some(clip_end) = &self.clip_end {
            write!(f, " clipEnd=\"{}\"", clip_end)?;
        }
        if let Some(repeat_dur) = &self.repeat_dur {
            write!(f, " repeatDur=\"{}\"", repeat_dur)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::*;
    use assert_approx_eq::assert_approx_eq;
    use fake::{Fake, Faker};
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    #[test]
    fn duration_conversion() {
        let time = TimeDesignation::Seconds(2.0);
        let time_ms = TimeDesignation::Milliseconds(2000.0);
        assert_eq!(time.duration(), time_ms.duration());
    }

    // If we take one of our elements and write it out again in theory we should reparse it as the
    // same element!

    #[test]
    fn speak_conversions() {
        // lets try 30 times
        for _ in 0..30 {
            let speak: SpeakAttributes = Faker.fake();

            let xml = format!(
                "<{} {}></{}>",
                SsmlElement::Speak,
                speak.to_string(),
                SsmlElement::Speak
            );
            println!("{}", xml);

            let mut reader = Reader::from_reader(xml.as_ref());
            let event = reader.read_event().unwrap();
            println!("{:?}", event);
            if let Event::Start(bs) = event {
                let (ssml_element, parsed_element) = parse_element(bs, &mut reader).unwrap();

                assert_eq!(ssml_element, SsmlElement::Speak);
                assert_eq!(parsed_element, ParsedElement::Speak(speak));
            } else {
                panic!("Didn't get expected event");
            }
        }
    }

    #[test]
    fn lang_conversions() {
        for _ in 0..30 {
            let lang: LangAttributes = Faker.fake();

            let xml = format!("<{} {}></{}>", SsmlElement::Lang, lang, SsmlElement::Lang);

            let mut reader = Reader::from_reader(xml.as_ref());
            let event = reader.read_event().unwrap();
            println!("{:?}", event);
            if let Event::Start(bs) = event {
                let (ssml_element, parsed_element) = parse_element(bs, &mut reader).unwrap();

                assert_eq!(ssml_element, SsmlElement::Lang);
                assert_eq!(parsed_element, ParsedElement::Lang(lang));
            } else {
                panic!("Didn't get expected event");
            }
        }
    }

    #[test]
    fn lookup_conversions() {
        for _ in 0..30 {
            let look: LookupAttributes = Faker.fake();

            let xml = format!(
                "<{} {}></{}>",
                SsmlElement::Lookup,
                look,
                SsmlElement::Lookup
            );

            let mut reader = Reader::from_reader(xml.as_ref());
            let event = reader.read_event().unwrap();
            println!("{:?}", event);
            if let Event::Start(bs) = event {
                let (ssml_element, parsed_element) = parse_element(bs, &mut reader).unwrap();

                assert_eq!(ssml_element, SsmlElement::Lookup);
                assert_eq!(parsed_element, ParsedElement::Lookup(look));
            } else {
                panic!("Didn't get expected event");
            }
        }
    }

    #[test]
    fn meta_conversions() {
        for _ in 0..30 {
            let meta: MetaAttributes = Faker.fake();

            let xml = format!("<{} {}></{}>", SsmlElement::Meta, meta, SsmlElement::Meta);

            let mut reader = Reader::from_reader(xml.as_ref());
            let event = reader.read_event().unwrap();
            println!("{:?}", event);
            if let Event::Start(bs) = event {
                let (ssml_element, parsed_element) = parse_element(bs, &mut reader).unwrap();

                assert_eq!(ssml_element, SsmlElement::Meta);
                assert_eq!(parsed_element, ParsedElement::Meta(meta));
            } else {
                panic!("Didn't get expected event");
            }
        }
    }

    #[test]
    fn token_conversions() {
        for _ in 0..30 {
            let token: TokenAttributes = Faker.fake();

            let xml = format!(
                "<{} {}></{}>",
                SsmlElement::Token,
                token,
                SsmlElement::Token
            );

            let mut reader = Reader::from_reader(xml.as_ref());
            let event = reader.read_event().unwrap();
            println!("{:?}", event);
            if let Event::Start(bs) = event {
                let (ssml_element, parsed_element) = parse_element(bs, &mut reader).unwrap();

                assert_eq!(ssml_element, SsmlElement::Token);
                assert_eq!(parsed_element, ParsedElement::Token(token.clone()));
            } else {
                panic!("Didn't get expected token event");
            }

            let xml = format!("<{} {}></{}>", SsmlElement::Word, token, SsmlElement::Word);

            let mut reader = Reader::from_reader(xml.as_ref());
            let event = reader.read_event().unwrap();
            println!("{:?}", event);
            if let Event::Start(bs) = event {
                let (ssml_element, parsed_element) = parse_element(bs, &mut reader).unwrap();

                assert_eq!(ssml_element, SsmlElement::Word);
                assert_eq!(parsed_element, ParsedElement::Word(token));
            } else {
                panic!("Didn't get expected word event");
            }
        }
    }

    #[test]
    fn say_as_conversions() {
        for _ in 0..30 {
            let say_as: SayAsAttributes = Faker.fake();

            let xml = format!(
                "<{} {}></{}>",
                SsmlElement::SayAs,
                say_as,
                SsmlElement::SayAs
            );

            let mut reader = Reader::from_reader(xml.as_ref());
            let event = reader.read_event().unwrap();
            println!("{:?}", event);
            if let Event::Start(bs) = event {
                let (ssml_element, parsed_element) = parse_element(bs, &mut reader).unwrap();

                assert_eq!(ssml_element, SsmlElement::SayAs);
                assert_eq!(parsed_element, ParsedElement::SayAs(say_as));
            } else {
                panic!("Didn't get expected event");
            }
        }
    }

    #[test]
    fn phoneme_conversions() {
        for _ in 0..30 {
            let attr: PhonemeAttributes = Faker.fake();

            let xml = format!(
                "<{} {}></{}>",
                SsmlElement::Phoneme,
                attr,
                SsmlElement::Phoneme
            );

            let mut reader = Reader::from_reader(xml.as_ref());
            let event = reader.read_event().unwrap();
            println!("{:?}", event);
            if let Event::Start(bs) = event {
                let (ssml_element, parsed_element) = parse_element(bs, &mut reader).unwrap();

                assert_eq!(ssml_element, SsmlElement::Phoneme);
                assert_eq!(parsed_element, ParsedElement::Phoneme(attr));
            } else {
                panic!("Didn't get expected event");
            }
        }
    }

    #[test]
    fn break_conversions() {
        for _ in 0..30 {
            let attr: BreakAttributes = Faker.fake();

            let xml = format!("<{} {}></{}>", SsmlElement::Break, attr, SsmlElement::Break);

            let mut reader = Reader::from_reader(xml.as_ref());
            let event = reader.read_event().unwrap();
            println!("{:?}", event);
            if let Event::Start(bs) = event {
                let (ssml_element, parsed_element) = parse_element(bs, &mut reader).unwrap();

                assert_eq!(ssml_element, SsmlElement::Break);
                assert_eq!(parsed_element, ParsedElement::Break(attr));
            } else {
                panic!("Didn't get expected event");
            }
        }
    }

    #[test]
    fn prosody_conversions() {
        // Prosody has a lot more area to cover!
        for _ in 0..50 {
            let attr: ProsodyAttributes = Faker.fake();

            let xml = format!(
                "<{} {}></{}>",
                SsmlElement::Prosody,
                attr,
                SsmlElement::Prosody
            );

            println!("{}", xml);

            let mut reader = Reader::from_reader(xml.as_ref());
            let event = reader.read_event().unwrap();
            println!("{:?}", event);
            if let Event::Start(bs) = event {
                let (ssml_element, parsed_element) = parse_element(bs, &mut reader).unwrap();

                assert_eq!(ssml_element, SsmlElement::Prosody);
                assert_eq!(parsed_element, ParsedElement::Prosody(attr));
            } else {
                panic!("Didn't get expected event");
            }
        }
    }

    #[test]
    fn mark_conversions() {
        for _ in 0..30 {
            let attr: MarkAttributes = Faker.fake();

            let xml = format!("<{} {}></{}>", SsmlElement::Mark, attr, SsmlElement::Mark);

            let mut reader = Reader::from_reader(xml.as_ref());
            let event = reader.read_event().unwrap();
            println!("{:?}", event);
            if let Event::Start(bs) = event {
                let (ssml_element, parsed_element) = parse_element(bs, &mut reader).unwrap();

                assert_eq!(ssml_element, SsmlElement::Mark);
                assert_eq!(parsed_element, ParsedElement::Mark(attr));
            } else {
                panic!("Didn't get expected event");
            }
        }
    }

    #[test]
    fn emphasis_conversions() {
        for _ in 0..30 {
            let attr: EmphasisAttributes = Faker.fake();

            let xml = format!(
                "<{} {}></{}>",
                SsmlElement::Emphasis,
                attr,
                SsmlElement::Emphasis
            );

            let mut reader = Reader::from_reader(xml.as_ref());
            let event = reader.read_event().unwrap();
            println!("{:?}", event);
            if let Event::Start(bs) = event {
                let (ssml_element, parsed_element) = parse_element(bs, &mut reader).unwrap();

                assert_eq!(ssml_element, SsmlElement::Emphasis);
                assert_eq!(parsed_element, ParsedElement::Emphasis(attr));
            } else {
                panic!("Didn't get expected event");
            }
        }
    }

    #[test]
    fn sub_conversions() {
        for _ in 0..30 {
            let attr: SubAttributes = Faker.fake();

            let xml = format!("<{} {}></{}>", SsmlElement::Sub, attr, SsmlElement::Sub);

            let mut reader = Reader::from_reader(xml.as_ref());
            let event = reader.read_event().unwrap();
            println!("{:?}", event);
            if let Event::Start(bs) = event {
                let (ssml_element, parsed_element) = parse_element(bs, &mut reader).unwrap();

                assert_eq!(ssml_element, SsmlElement::Sub);
                assert_eq!(parsed_element, ParsedElement::Sub(attr));
            } else {
                panic!("Didn't get expected event");
            }
        }
    }

    #[test]
    fn lexicon_conversions() {
        for _ in 0..30 {
            let attr: LexiconAttributes = Faker.fake();

            let xml = format!(
                "<{} {}></{}>",
                SsmlElement::Lexicon,
                attr,
                SsmlElement::Lexicon
            );

            let mut reader = Reader::from_reader(xml.as_ref());
            let event = reader.read_event().unwrap();
            println!("{:?}", event);
            if let Event::Start(bs) = event {
                let (ssml_element, parsed_element) = parse_element(bs, &mut reader).unwrap();

                assert_eq!(ssml_element, SsmlElement::Lexicon);
                assert_eq!(parsed_element, ParsedElement::Lexicon(attr));
            } else {
                panic!("Didn't get expected event");
            }
        }
    }

    #[test]
    fn voice_conversions() {
        for _ in 0..30 {
            let attr: VoiceAttributes = Faker.fake();

            let xml = format!("<{} {}></{}>", SsmlElement::Voice, attr, SsmlElement::Voice);

            let mut reader = Reader::from_reader(xml.as_ref());
            let event = reader.read_event().unwrap();
            println!("{:?}", event);
            if let Event::Start(bs) = event {
                let (ssml_element, parsed_element) = parse_element(bs, &mut reader).unwrap();

                assert_eq!(ssml_element, SsmlElement::Voice);
                assert_eq!(parsed_element, ParsedElement::Voice(attr));
            } else {
                panic!("Didn't get expected event");
            }
        }
    }

    #[test]
    fn audio_conversions() {
        for _ in 0..50 {
            let attr: AudioAttributes = Faker.fake();

            let xml = format!("<{} {}></{}>", SsmlElement::Audio, attr, SsmlElement::Audio);

            let mut reader = Reader::from_reader(xml.as_ref());
            let event = reader.read_event().unwrap();
            println!("{:?}", event);
            if let Event::Start(bs) = event {
                let (ssml_element, parsed_element) = parse_element(bs, &mut reader).unwrap();

                assert_eq!(ssml_element, SsmlElement::Audio);
                if let ParsedElement::Audio(parsed) = parsed_element {
                    assert_eq!(parsed.src, attr.src);
                    assert_eq!(parsed.fetch_timeout, attr.fetch_timeout);
                    assert_eq!(parsed.fetch_hint, attr.fetch_hint);
                    assert_eq!(parsed.max_age, attr.max_age);
                    assert_eq!(parsed.max_stale, attr.max_stale);
                    assert_eq!(parsed.clip_begin, attr.clip_begin);
                    assert_eq!(parsed.clip_end, attr.clip_end);
                    assert_eq!(parsed.repeat_count, attr.repeat_count);
                    assert_eq!(parsed.repeat_dur, attr.repeat_dur);
                    assert_approx_eq!(parsed.sound_level, attr.sound_level);
                    assert_approx_eq!(parsed.speed, attr.speed);
                } else {
                    panic!(
                        "SSML Element type doesn't match actual parsed value: {:?}",
                        parsed_element
                    );
                }
            } else {
                panic!("Didn't get expected event");
            }
        }
    }
}
