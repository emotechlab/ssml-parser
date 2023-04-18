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

// p, audio, break, emphasis, lang, lookup, mark, phoneme, prosody, say-as, sub, s, token, voice, w.
//
// p
// Audio https://www.w3.org/TR/speech-synthesis11/#edef_audio

// Speak can contain:
// * audio - allows for inserting prerecorded audio into output
// * break - controls the pausing or other prosodic boundaries between tokens
// * emphasis - requests that the contained text be spoken with emphasis (also referred to as prominence or stress)
// * lang
// * lexicon
// * lookup
// * mark
// * meta - metadata associated with document
// * metadata - recommended to be RDF about the properties/relationships in document
// * p - Paragraph to speak
// * phoneme - provides a phonemic/phonetic pronunciation for the contained text
// * prosody - permits control of the pitch, speaking rate and volume of the speech output. The attributes, all optional, are:
// * say-as
// * sub
// * s - Sentence to speak
// * token - help segmentation of languages that don't separate via whitespace or things like syllables (can apply expression to just a syllable_
// * voice - production element that requests a change in speaking voice
// * w
//
//
// The token element can only be contained in the following elements: audio, emphasis, lang, lookup, prosody, speak, p, s, voice.
//
// The say-as element has three attributes: interpret-as, format, and detail. The interpret-as attribute is always required; the other two attributes are optional. The legal values for the format attribute depend on the value of the interpret-as attribute.
use anyhow::bail;
use std::collections::HashMap;
use std::convert::Infallible;
use std::num::NonZeroUsize;
use std::str::FromStr;
use std::time::Duration;

// Structural elements
// * speak
// * lexicon
// * lookup
// * meta
// * metadata
// * p/s/token/word
// * say-as
// * phoneme
// * sub
// * lang
//

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum SsmlElement {
    Speak,
    Lexicon,
    Lookup,
    Meta,
    Metadata,
    Paragraph,
    Sentence,
    Token,
    Word,
    SayAs,
    Phoneme,
    Sub,
    Lang,
    Voice,
    Emphasis,
    Break,
    Prosody,
    Audio,
    Mark,
    Description,
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

    #[inline(always)]
    fn allowed_in_paragraph(&self) -> bool {
        matches!(self, Self::Sentence) || self.allowed_in_sentence()
    }

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
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedElement {
    Speak(SpeakAttributes),
    // TODO: spec mentions `lexicon` can only be immediate children of `speak`. enforce this check
    Lexicon(LexiconAttributes),
    Lookup(LookupAttributes),
    Meta(MetaAttributes),
    Metadata,
    Paragraph,
    Sentence,
    Token(TokenAttributes),
    // `w` element is just an alias for `token`
    Word(TokenAttributes),
    SayAs(SayAsAttributes),
    Phoneme(PhonemeAttributes),
    Sub(SubAttributes),
    Lang(LangAttributes),
    Voice(VoiceAttributes),
    Emphasis(EmphasisAttributes),
    Break(BreakAttributes),
    Prosody(ProsodyAttributes),
    Audio,
    Mark,
    Description,
    Custom((String, HashMap<String, String>)),
}

impl ParsedElement {
    pub fn can_contain_tags(&self) -> bool {
        SsmlElement::from(self).can_contain_tags()
    }

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
            ParsedElement::Audio => Self::Audio,
            ParsedElement::Mark => Self::Mark,
            ParsedElement::Description => Self::Description,
            ParsedElement::Custom((s, _)) => Self::Custom(s.to_string()),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SpeakAttributes {
    pub lang: Option<String>,
    pub base: Option<String>,
    pub on_lang_failure: Option<OnLanguageFailure>,
}

/// The lang element is used to specify the natural language of the content.
#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct LangAttributes {
    pub lang: String,
    pub on_lang_failure: Option<OnLanguageFailure>,
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
            "description" => Self::Description,
            e => Self::Custom(e.to_string()),
        };
        Ok(s)
    }
}

// Prosody and style
// * voice
// * emphasis
// * break
// * prosody

// Other
// * audio
// * mark
// * desc

// Custom

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
    pub uri: http::Uri,
    pub xml_id: String,
    pub ty: Option<mediatype::MediaTypeBuf>,
    pub fetchtimeout: Option<TimeDesignation>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TimeDesignation {
    Seconds(f32),
    Milliseconds(f32),
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
pub struct LookupAttributes {
    pub lookup_ref: String,
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
    pub name: Option<String>,
    pub http_equiv: Option<String>,
    pub content: String,
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

/// The say-as element allows the author to indicate information on the type of text
/// construct contained within the element and to help specify the level of detail
/// for rendering the contained text.
/// The say-as element has three attributes: interpret-as, format, and detail.
/// The interpret-as attribute is always required; the other two attributes are optional.
/// The legal values for the format attribute depend on the value of the interpret-as attribute.
/// The say-as element can only contain text to be rendered.
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Debug, Clone, Eq, PartialEq)]
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

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum PhonemeAlphabet {
    Ipa,
    Other(String),
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

impl FromStr for Strength {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
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

/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
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

/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
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

/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub enum PitchRange {
    Strength(PitchStrength), // low, medium high etc
    Frequency((f32, Unit)),
    RelativeChange((f32, Unit)),
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
            value if value.ends_with("Hz") || value.ends_with("%") || value.ends_with("st") => {
                if value.ends_with("Hz") {
                    if value.starts_with("+") || value.starts_with("-") {
                        if value.starts_with("-") {
                            Ok(Self::RelativeChange((
                                value.strip_suffix("Hz").unwrap().parse::<f32>()?,
                                Unit::Hz,
                            )))
                        } else {
                            Ok(Self::RelativeChange((
                                value.strip_suffix("Hz").unwrap().parse()?,
                                Unit::Hz,
                            )))
                        }
                    } else {
                        Ok(Self::Frequency((
                            value.strip_suffix("Hz").unwrap().parse()?,
                            Unit::Hz,
                        )))
                    }
                } else if value.ends_with("%") {
                    if value.starts_with("+") || value.starts_with("-") {
                        if value.starts_with("-") {
                            Ok(Self::RelativeChange((
                                value.strip_suffix("%").unwrap().parse::<f32>()?,
                                Unit::Percentage,
                            )))
                        } else {
                            Ok(Self::RelativeChange((
                                value.strip_suffix("%").unwrap().parse()?,
                                Unit::Percentage,
                            )))
                        }
                    } else {
                        Ok(Self::RelativeChange((
                            value.strip_suffix("%").unwrap().parse()?,
                            Unit::Percentage,
                        )))
                    }
                } else if value.ends_with("st") {
                    if value.starts_with("+") || value.starts_with("-") {
                        if value.starts_with("-") {
                            Ok(Self::RelativeChange((
                                value.strip_suffix("st").unwrap().parse::<f32>()?,
                                Unit::St,
                            )))
                        } else {
                            Ok(Self::RelativeChange((
                                value.strip_suffix("st").unwrap().parse()?,
                                Unit::St,
                            )))
                        }
                    } else {
                        Ok(Self::RelativeChange((
                            value.strip_suffix("st").unwrap().parse()?,
                            Unit::St,
                        )))
                    }
                } else {
                    bail!("Unrecognised value {}", "Pitch value unrecognised");
                }
            }
            e => bail!("Unrecognised value {}", e),
        }
    }
}

/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub enum VolumeRange {
    Strength(VolumeStrength), // "silent", "x-soft", "soft", "medium", "loud", "x-loud", default
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
            value if value.ends_with("dB") => {
                if value.starts_with("+") || value.starts_with("-") {
                    if value.starts_with("-") {
                        Ok(Self::Decibel(
                            value.strip_suffix("dB").unwrap().parse::<f32>()? * -1.0,
                        ))
                    } else {
                        Ok(Self::Decibel(
                            value.strip_suffix("dB").unwrap().parse::<f32>()?,
                        ))
                    }
                } else {
                    Ok(Self::Decibel(
                        value.strip_suffix("dB").unwrap().parse::<f32>()?,
                    ))
                }
            }
            e => bail!("Unrecognised value {}", e),
        }
    }
}

/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub enum RateRange {
    Strength(RateStrength), // "x-slow", "slow", "medium", "fast", "x-fast", or "default"
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
            value if value.ends_with("%") => {
                if value.starts_with("+") || value.starts_with("-") {
                    if value.starts_with("+") {
                        Ok(Self::Percentage(
                            value.strip_suffix("%").unwrap().parse::<PositiveNumber>()?,
                        ))
                    } else {
                        bail!(
                            "Unrecognised value {}",
                            "Negative percentage not allowed for rate"
                        );
                    }
                } else {
                    Ok(Self::Percentage(
                        value.strip_suffix("%").unwrap().parse::<PositiveNumber>()?,
                    ))
                }
            }
            e => bail!("Unrecognised value {}", e),
        }
    }
}

/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub enum ContourElement {
    Element((f32, PitchRange)),
}

impl FromStr for ContourElement {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            value if value.starts_with("(") && value.ends_with(")") => {
                let value = value.strip_suffix(")").unwrap().to_string();
                let value = value.strip_prefix("(").unwrap().to_string();
                let elements = value.split(",").collect::<Vec<_>>();

                let pitch = match PitchRange::from_str(&elements[1]) {
                    Ok(result) => result,
                    Err(e) => bail!("Error: {}", e),
                };

                if elements[0].ends_with("%") {
                    let percentage = elements[0].strip_suffix("%").unwrap().parse::<f32>()?;
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

/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub enum PitchContour {
    Elements(Vec<ContourElement>),
}

impl FromStr for PitchContour {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut pitch_contour_elements = Vec::new();
        match s {
            value if value.starts_with("(") => {
                let elements = value.split(" ").collect::<Vec<_>>();

                for element in elements {
                    let pitchcontourelement = ContourElement::from_str(&element)?;
                    pitch_contour_elements.push(pitchcontourelement);
                }

                Ok(Self::Elements(pitch_contour_elements))
            }
            e => bail!("Unrecognised value {}", e),
        }
    }
}

/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub enum PositiveNumber {
    FloatNumber(f32),
    RoundNumber(isize),
}

impl FromStr for PositiveNumber {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            value
                if value.starts_with("+")
                    || value.starts_with("-")
                    || value.parse::<f32>().is_ok() =>
            {
                if value.starts_with("+") {
                    if value.contains(".") {
                        Ok(Self::FloatNumber(
                            value.strip_prefix("+").unwrap().parse::<f32>()?,
                        ))
                    } else {
                        Ok(Self::RoundNumber(
                            value.strip_prefix("+").unwrap().parse::<isize>()?,
                        ))
                    }
                } else {
                    if value.starts_with("-") {
                        bail!("Unrecognised value {}", "Negative number not allowed");
                    } else {
                        if value.contains(".") {
                            Ok(Self::FloatNumber(value.parse::<f32>()?))
                        } else {
                            Ok(Self::RoundNumber(value.parse::<isize>()?))
                        }
                    }
                }
            }
            e => bail!("Unrecognised value {}", e),
        }
    }
}

/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Unit {
    /// Strong
    Hz,
    /// Moderate (default)
    St,
    /// None
    Percentage,
}

/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
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
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
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
    pub time: Option<Duration>,
}
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Clone, Debug, PartialEq, PartialOrd)]
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
    pub duration: Option<Duration>,
    /// the volume for the contained text. Legal values are: a number preceded by "+" or "-"
    /// and immediately followed by "dB"; or "silent", "x-soft", "soft", "medium", "loud", "x-loud",
    /// or "default". The default is +0.0dB. Specifying a value of "silent" amounts to specifying
    /// minus infinity decibels (dB). Labels "silent" through "x-loud" represent a sequence of
    /// monotonically non-decreasing volume levels. When the value is a signed number (dB),
    /// it specifies the ratio of the squares of the new signal amplitude (a1) and the current
    /// amplitude (a0), and is defined in terms of dB:
    pub volume: Option<VolumeRange>,
}

/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
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
pub struct SubAttributes {
    pub alias: String,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Gender {
    Male,
    Female,
    Neutral,
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
pub struct LanguageAccentPair {
    pub lang: String,
    pub accent: Option<String>,
}

impl FromStr for LanguageAccentPair {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            bail!("Empty language string");
        } else if s == "und" || s == "zxx" {
            bail!("Disallowed language code");
        } else {
            let lang_accent = s.split(":").collect::<Vec<_>>();
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
pub struct VoiceAttributes {
    pub gender: Option<Gender>,
    pub age: Option<u8>,
    pub variant: Option<NonZeroUsize>,
    pub name: Vec<String>,
    pub languages: Vec<LanguageAccentPair>,
}
