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

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ParsedElement {
    Speak(SpeakAttributes),
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
    Custom((String, HashMap<String, String>)),
}

impl From<&ParsedElement> for SsmlElement {
    fn from(elem: &ParsedElement) -> Self {
        match elem {
            ParsedElement::Speak(_) => Self::Speak,
            ParsedElement::Lexicon => Self::Lexicon,
            ParsedElement::Lookup => Self::Lookup,
            ParsedElement::Meta => Self::Meta,
            ParsedElement::Metadata => Self::Metadata,
            ParsedElement::Paragraph => Self::Paragraph,
            ParsedElement::Sentence => Self::Sentence,
            ParsedElement::Token => Self::Token,
            ParsedElement::Word => Self::Word,
            ParsedElement::SayAs => Self::SayAs,
            ParsedElement::Phoneme => Self::Phoneme,
            ParsedElement::Sub => Self::Sub,
            ParsedElement::Lang => Self::Lang,
            ParsedElement::Voice => Self::Voice,
            ParsedElement::Emphasis => Self::Emphasis,
            ParsedElement::Break => Self::Break,
            ParsedElement::Prosody => Self::Prosody,
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

pub enum PhonemeAlphabet {
    Ipa,
    Other(String),
}

/// The phoneme element provides a phonemic/phonetic pronunciation for the
/// contained text. The phoneme element may be empty. However, it is recommended
/// that the element contain human-readable text that can be used for non-spoken
/// rendering of the document. For example, the content may be displayed visually
/// for users with hearing impairments.
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
pub struct Phoneme {
    /// The ph attribute is a required attribute that specifies the phoneme/phone
    /// string.
    ph: String,
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
    alphabet: Option<PhonemeAlphabet>,
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
#[derive(Copy, Clone, Debug)]
pub struct Break {
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
    strength: Option<Strength>,
    /// The time attribute is an optional attribute indicating the duration of a
    /// pause to be inserted in the output in seconds or milliseconds. It
    /// follows the time value format from the Cascading Style Sheets Level 2
    /// Recommendation [CSS2], e.g. "250ms",
    time: Duration,
}
