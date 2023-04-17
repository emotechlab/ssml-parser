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
    Lookup,
    Meta,
    Metadata,
    Paragraph,
    Sentence,
    Token,
    Word,
    SayAs(SayAsAttributes),
    Phoneme(PhonemeAttributes),
    Sub,
    Lang,
    Voice,
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
            ParsedElement::Lookup => Self::Lookup,
            ParsedElement::Meta => Self::Meta,
            ParsedElement::Metadata => Self::Metadata,
            ParsedElement::Paragraph => Self::Paragraph,
            ParsedElement::Sentence => Self::Sentence,
            ParsedElement::Token => Self::Token,
            ParsedElement::Word => Self::Word,
            ParsedElement::SayAs(_) => Self::SayAs,
            ParsedElement::Phoneme(_) => Self::Phoneme,
            ParsedElement::Sub => Self::Sub,
            ParsedElement::Lang => Self::Lang,
            ParsedElement::Voice => Self::Voice,
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
    pub on_lang_failure: Option<String>, // TODO make into OnLanguageFailure
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
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = match s {
            "changevoice" => Self::ChangeVoice,
            "ignoretext" => Self::IgnoreText,
            "ignorelang" => Self::IgnoreLang,
            "processorchoice" => Self::ProcessorChoice,
            _ => todo!(),
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
            e => bail!("Unrecognised value {}", e),
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
                let mut percentage = 0.0;
                if elements[0].ends_with("%") {
                    percentage = elements[0].strip_suffix("%").unwrap().parse::<f32>()?;
                } else {
                    bail!(
                        "Unrecognised value {}",
                        "Invalid percentage in pitch contour"
                    );
                }
                let pitch = match PitchRange::from_str(&elements[1]) {
                    Ok(result) => result,
                    Err(e) => bail!("Error: {}", e),
                };

                Ok(Self::Element((percentage, pitch)))
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
        let mut pitchContourElements = Vec::new();
        match s {
            value if value.starts_with("(") => {
                let elements = value.split(" ").collect::<Vec<_>>();

                for element in elements {
                    let pitchcontourelement = ContourElement::from_str(&element)?;
                    pitchContourElements.push(pitchcontourelement);
                }

                Ok(Self::Elements(pitchContourElements))
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
