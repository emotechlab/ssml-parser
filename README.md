# SSML Parser

This crate handles parsing SSML (Speech Synthesis Markup Language). It's main
aim is to facilitate the development of TTS (Text-To-Speech) and applications
that utilise sythesised audio. 

Currently it contains a full implementation of the SSML 1.1 specification
including custom tags. Text within custom tags is assumed to be synthesisable
though it is possible to change this behaviour when extracting the text.
