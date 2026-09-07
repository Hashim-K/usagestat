# Dependency notices missing from crate archives

These files fill omissions in the exact locked registry archives. They are
copied verbatim from each crate's `.cargo_vcs_info.json` revision; the manifest
records upstream URLs, package versions, declared licenses and SHA-256 digests.
The packager validates these before adding the text to the archive's combined
LICENSE. It makes no network request for license text at build or install time.
New dependency versions require a fresh review; a version/license/repository
mismatch or modified notice fails packaging.

`age` and `age-core` keep licenses in their upstream workspace root.
`cookie-factory` uses REUSE's `LICENSES/MIT.txt` plus `.reuse/dep5` attribution.
`fluent-langneg` and `intl_pluralrules` omit their upstream license files from
published crate archives. Both available MIT/Apache texts are retained.

`io_tee` 0.1.1 declares MIT OR Apache-2.0 in Cargo metadata and its pinned README,
but the upstream revision contains no standalone license text. This distribution
uses its Apache-2.0 option: the README declaration and the Apache Software
Foundation's complete version 2.0 text are bundled together. No copyright notice
has been invented or substituted.
