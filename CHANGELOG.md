# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Fixed
- This fixes a small issue with the regex used to extract the time for e.g. break tags.
### Added 
- Added attributes for say-as, prosody and emphasis tags and functions for parsing those
- Added basic parsing of elements with no extracting of attributes for standard elements
- Extraction of attributes for custom elements
- Full support for `<break/>` element
- Full support for `<phoneme>` elements
- Character position reporting for spans (not byte or grapheme)
- Reject invalid nesting of elements and add API functions too check if elements can be nested
- Description element text is now ignored
- Ability to expand sub elements during SSML parsing
