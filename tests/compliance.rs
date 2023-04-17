use ssml_parser::elements::*;
use ssml_parser::parser::parse_ssml;
use std::time::Duration;

/// Example SSML taken from Appendix E in the SSML specification which
/// can be found [here](https://www.w3.org/TR/speech-synthesis11). All copied
/// sections will be marked with:
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[test]
fn simple_example() {
    let ssml = r#"<?xml version="1.0"?>
        <speak version="1.1"
               xmlns="http://www.w3.org/2001/10/synthesis"
               xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
               xsi:schemaLocation="http://www.w3.org/2001/10/synthesis
                           http://www.w3.org/TR/speech-synthesis11/synthesis.xsd"
               xml:lang="en-US">
          <p>
            <s>You have 4 new messages.</s>
            <s>The first is from Stephanie Williams and arrived at <break/> 3:45pm.
            </s>
            <s>
              The subject is <prosody rate="20%">ski trip</prosody>
            </s>
          </p>
        </speak>"#;

    let result = parse_ssml(ssml).unwrap();

    let whole_sentence = "You have 4 new messages. The first is from Stephanie Williams and arrived at 3:45pm. The subject is ski trip";

    assert_eq!(result.get_text().trim(), whole_sentence);

    let tags = result.tags().collect::<Vec<_>>();
    assert_eq!(tags.len(), 7);

    if let ParsedElement::Speak(s) = &tags[0].element {
        assert_eq!(s.lang.as_ref().unwrap(), "en-US");
        assert_eq!(result.get_text_from_span(&tags[0]).trim(), whole_sentence);
    } else {
        panic!("Tag 0 wrong: {:?}", tags[0]);
    }

    if let ParsedElement::Paragraph = &tags[1].element {
        assert_eq!(result.get_text_from_span(&tags[1]).trim(), whole_sentence);
    } else {
        panic!("Tag 1 wrong: {:?}", tags[1]);
    }

    if let ParsedElement::Sentence = &tags[2].element {
        assert_eq!(
            result.get_text_from_span(&tags[2]).trim(),
            "You have 4 new messages."
        );
    } else {
        panic!("Tag 2 wrong: {:?}", tags[2]);
    }

    if let ParsedElement::Sentence = &tags[3].element {
        assert_eq!(
            result.get_text_from_span(&tags[3]).trim(),
            "The first is from Stephanie Williams and arrived at 3:45pm."
        );
    } else {
        panic!("Tag 3 wrong: {:?}", tags[3]);
    }

    if let ParsedElement::Break(b) = tags[4].element {
        assert_eq!(b.strength, None);
        assert_eq!(b.time, None);
    } else {
        panic!("Tag 4 wrong {:?}", tags[4]);
    }

    if let ParsedElement::Sentence = &tags[5].element {
        assert_eq!(
            result.get_text_from_span(&tags[5]).trim(),
            "The subject is ski trip"
        );
    } else {
        panic!("Tag 5 wrong: {:?}", tags[5]);
    }

    if let ParsedElement::Prosody(p) = &tags[6].element {
        assert_eq!(p.pitch, None);
        assert_eq!(p.contour, None);
        assert_eq!(p.range, None);
        assert_eq!(
            p.rate,
            Some(RateRange::Percentage(PositiveNumber::RoundNumber(20)))
        );
        assert_eq!(p.duration, None);
        assert_eq!(p.volume, None);
    } else {
        panic!("Tag 6 wrong: {:?}", tags[6]);
    }

    //    todo!()
}

/// Example SSML taken from Appendix E in the SSML specification which
/// can be found [here](https://www.w3.org/TR/speech-synthesis11). All copied
/// sections will be marked with:
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[test]
fn audio_example() {
    let ssml = r#"<?xml version="1.0"?>
        <speak version="1.1"
               xmlns="http://www.w3.org/2001/10/synthesis"
               xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
               xsi:schemaLocation="http://www.w3.org/2001/10/synthesis
                           http://www.w3.org/TR/speech-synthesis11/synthesis.xsd"
               xml:lang="en-US">

          <p>
            <voice gender="male">
              <s>Today we preview the latest romantic music from Example.</s>

              <s>Hear what the Software Reviews said about Example's newest hit.</s>
            </voice>
          </p>

          <p>
            <voice gender="female">
              He sings about issues that touch us all.
            </voice>
          </p>

          <p>
            <voice gender="male">
              Here's a sample.  <audio src="http://www.example.com/music.wav"/>
              Would you like to buy it?
            </voice>
          </p>

        </speak>
        "#;
    let result = parse_ssml(ssml).unwrap();
    assert_eq!(result.get_text().trim(),
               "Today we preview the latest romantic music from Example. Hear what the Software Reviews said about Example's newest hit. He sings about issues that touch us all. Here's a sample. Would you like to buy it?");

    //todo!()
}

/// Example SSML taken from Appendix E in the SSML specification which
/// can be found [here](https://www.w3.org/TR/speech-synthesis11). All copied
/// sections will be marked with:
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[test]
fn mixed_language_example() {
    let ssml = r#"<?xml version="1.0" encoding="ISO-8859-1"?>
        <speak version="1.1" xmlns="http://www.w3.org/2001/10/synthesis"
               xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
               xsi:schemaLocation="http://www.w3.org/2001/10/synthesis
                         http://www.w3.org/TR/speech-synthesis11/synthesis.xsd"
               xml:lang="en-US">
          
          The title of the movie is:
          "La vita è bella"
          (Life is beautiful),
          which is directed by Roberto Benigni.
        </speak>"#;
    let result = parse_ssml(ssml).unwrap();
    assert_eq!(
        result.get_text().trim(),
        r#"The title of the movie is: "La vita è bella" (Life is beautiful), which is directed by Roberto Benigni."#
    );
}

/// Example SSML taken from Appendix E in the SSML specification which
/// can be found [here](https://www.w3.org/TR/speech-synthesis11). All copied
/// sections will be marked with:
///
/// "Speech Synthesis Markup Language (SSML) Version 1.1" _Copyright © 2010 W3C® (MIT, ERCIM, Keio),
/// All Rights Reserved._
#[test]
fn ipa_support() {
    let ssml = r#"<?xml version="1.0" encoding="ISO-8859-1"?>
        <speak version="1.1" xmlns="http://www.w3.org/2001/10/synthesis"
               xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
               xsi:schemaLocation="http://www.w3.org/2001/10/synthesis
                         http://www.w3.org/TR/speech-synthesis11/synthesis.xsd"
               xml:lang="en-US">
          
          The title of the movie is: 
          <phoneme alphabet="ipa"
            ph="&#x2C8;l&#x251; &#x2C8;vi&#x2D0;&#x27E;&#x259; &#x2C8;&#x294;e&#x26A; &#x2C8;b&#x25B;l&#x259;"> 
          La vita è bella </phoneme>
          <!-- The IPA pronunciation is ˈlɑ ˈviːɾə ˈʔeɪ ˈbɛlə -->
          (Life is beautiful), 
          which is directed by 
          <phoneme alphabet="ipa"
            ph="&#x279;&#x259;&#x2C8;b&#x25B;&#x2D0;&#x279;&#x27E;o&#x28A; b&#x25B;&#x2C8;ni&#x2D0;nji"> 
          Roberto Benigni </phoneme>
          <!-- The IPA pronunciation is ɹəˈbɛːɹɾoʊ bɛˈniːnji -->

          <!-- Note that in actual practice an author might change the
             encoding to UTF-8 and directly use the Unicode characters in
             the document rather than using the escapes as shown.
             The escaped values are shown for ease of copying. -->
        </speak>"#;
    let result = parse_ssml(ssml).unwrap();
    assert_eq!(
        result.get_text().trim(),
        r#"The title of the movie is: La vita è bella (Life is beautiful), which is directed by Roberto Benigni"#
    );

    let phonemes = vec![
        (Some(PhonemeAlphabet::Ipa), "ˈlɑ ˈviːɾə ˈʔeɪ ˈbɛlə"),
        (Some(PhonemeAlphabet::Ipa), "ɹəˈbɛːɹɾoʊ bɛˈniːnji"),
    ];

    let mut index = 0;

    let tags: Vec<SsmlElement> = {
        use SsmlElement::*;
        vec![Speak, Phoneme, Phoneme]
    };

    for (parsed, expected) in result.tags().zip(tags.iter()) {
        let actual_tag = SsmlElement::from(&parsed.element);
        assert_eq!(actual_tag, *expected);

        if let ParsedElement::Phoneme(p) = &parsed.element {
            assert_eq!(p.alphabet, phonemes[index].0);
            assert_eq!(p.ph, phonemes[index].1);
            index += 1;
        }
    }
}

#[test]
fn google_tts_example() {
    let ssml = r#"<speak>
          Here are <say-as interpret-as="characters">SSML</say-as> samples.
          I can pause <break time="3s"/>.
          I can play a sound
          <audio src="https://www.example.com/MY_MP3_FILE.mp3">didn't get your MP3 audio file</audio>.
          I can speak in cardinals. Your number is <say-as interpret-as="cardinal">10</say-as>.
          Or I can speak in ordinals. You are <say-as interpret-as="ordinal">10</say-as> in line.
          Or I can even speak in digits. The digits for ten are <say-as interpret-as="characters">10</say-as>.
          I can also substitute phrases, like the <sub alias="World Wide Web Consortium">W3C</sub>.
          Finally, I can speak a paragraph with two sentences.
          <p><s>This is sentence one.</s><s>This is sentence two.</s></p>
        </speak>"#;
    let result = parse_ssml(ssml).unwrap();
    assert_eq!(
        result.get_text().trim(),
        r#"Here are SSML samples. I can pause . I can play a sound didn't get your MP3 audio file. I can speak in cardinals. Your number is 10. Or I can speak in ordinals. You are 10 in line. Or I can even speak in digits. The digits for ten are 10. I can also substitute phrases, like the W3C. Finally, I can speak a paragraph with two sentences. This is sentence one. This is sentence two."#
    );

    //  todo!();
}
#[test]
fn microsoft_custom_tags() {
    // I've had to modify the Microsoft example as:
    // 1. It was invalid XML (closing an already closed tag)
    // 2. Invalid parameter values that don't match the standard
    let ssml = r#"<speak version="1.0" xmlns="http://www.w3.org/2001/10/synthesis" xmlns:mstts="https://www.w3.org/2001/mstts" xml:lang="string">
    <mstts:backgroundaudio src="string" volume="string" fadein="string" fadeout="string"/>
    <voice name="string">
        <audio src="string"></audio>
        <bookmark mark="string"/>
        <break strength="medium" time="5s" />
        <emphasis level="reduced"></emphasis>
        <lang xml:lang="string"></lang>
        <lexicon xml:id="some_id" uri="string"/>
        <math xmlns="http://www.w3.org/1998/Math/MathML"></math>
        <mstts:express-as style="string" styledegree="value" role="string"></mstts:express-as>
        <mstts:silence type="string" value="string"/>
        <mstts:viseme type="string"/>
        <p></p>
        <phoneme alphabet="string" ph="string"></phoneme>
        <prosody pitch="2.2Hz" contour="(0%,+20Hz) (10%,+30Hz) (40%,+10Hz)" range="-2Hz" rate="20%" volume="2dB"></prosody>
        <s></s>
        <say-as interpret-as="string" format="string" detail="string"></say-as>
        <sub alias="string"></sub>
    </voice>
</speak>"#;
    let result = parse_ssml(ssml).unwrap();
    assert_eq!(result.get_text().trim(), "");

    let tags: Vec<SsmlElement> = {
        use SsmlElement::*;
        vec![
            Speak,
            Custom("backgroundaudio".to_string()),
            Voice,
            Audio,
            Custom("bookmark".to_string()),
            Break,
            Emphasis,
            Lang,
            Lexicon,
            Custom("math".to_string()),
            Custom("express-as".to_string()),
            Custom("silence".to_string()),
            Custom("viseme".to_string()),
            Paragraph,
            Phoneme,
            Prosody,
            Sentence,
            SayAs,
            Sub,
        ]
    };

    for (parsed, expected) in result.tags().zip(tags.iter()) {
        let actual_tag = SsmlElement::from(&parsed.element);
        assert_eq!(actual_tag, *expected);

        if let ParsedElement::Break(b) = parsed.element {
            assert_eq!(b.strength, Some(Strength::Medium));
            assert_eq!(b.time, Some(Duration::from_secs(5)));
        } else if let ParsedElement::Phoneme(p) = &parsed.element {
            assert_eq!(
                p.alphabet.as_ref().unwrap(),
                &PhonemeAlphabet::Other("string".to_string())
            );
            assert_eq!(p.ph, "string");
        }
    }
}
