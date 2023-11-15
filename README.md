# SSML Parser

This crate handles parsing SSML (Speech Synthesis Markup Language). It's main
aim is to facilitate the development of TTS (Text-To-Speech) and applications
that utilise sythesised audio. Functionality for writing XML is limited and 
could do with improvements for ergonomics.

Currently it contains a full implementation of the SSML 1.1 specification
including custom tags. Text within custom tags is assumed to be synthesisable
though it is possible to change this behaviour when extracting the text.

Below is a simple example:

```
use ssml_parser::parse_ssml;

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

// We can now see the text with tags removed:
println!("{}", result.get_text());

// And can loop over all the SSML tags and get their character indexes:
for tag in result.tags() {
   println!("{:?}", tag);
}
```
