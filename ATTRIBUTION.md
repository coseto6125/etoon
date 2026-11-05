# Attribution

## Test Fixtures

`tests/fixtures/encode/*.json` (except `key-folding.json` and
`spec-v4.1-legacy.json`) are copied from the
[toon-format/spec](https://github.com/toon-format/spec) repository at tag
`v4.4.0`, licensed under the MIT License. They are the official
language-agnostic encode fixture suite for TOON spec v4.4. No modifications
have been made to the fixture contents.

`spec-v4.1-legacy.json` holds the cases of the same suite at `v4.1.1` that
`v4.4.0` no longer lists (MIT License), unchanged except that each name carries
its source file.

`key-folding.json` is etoon-local. It started from the
[toons](https://github.com/alesanfra/toons) project by Alessandro Sanfratello
(Apache License 2.0) and carries etoon's own `@`/`$`/`#` sigil-prefix cases. The
spec removed key folding in v4.0, so this file guards etoon's `fold_keys`
extension rather than a spec requirement.

## TOON Specification

The TOON (Token-Oriented Object Notation) format is defined by
[toon-format/spec](https://github.com/toon-format/spec) (MIT License); the
reference TypeScript implementation lives in
[toon-format/toon](https://github.com/toon-format/toon) (MIT License).

## License

This project is licensed under the Apache License 2.0, which is compatible with
both the MIT-licensed spec fixtures and the Apache 2.0 `toons` fixtures it
reuses.
