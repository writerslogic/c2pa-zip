<p align="center">
  <h1 align="center">c2pa-zip</h1>
  <p align="center">C2PA Manifest Store embedding and reading for ZIP-based (OCF-style) documents: EPUB, DOCX, ODT, OXPS</p>
</p>

<p align="center">
  <a href="https://crates.io/crates/c2pa-zip"><img src="https://img.shields.io/crates/v/c2pa-zip.svg" alt="crates.io"></a>
  <a href="https://docs.rs/c2pa-zip"><img src="https://docs.rs/c2pa-zip/badge.svg" alt="docs.rs"></a>
  <a href="#license"><img src="https://img.shields.io/crates/l/c2pa-zip.svg" alt="License"></a>
</p>

## Overview

Implements the **ZIP embedding** method from the [C2PA Technical Specification](https://c2pa.org/specifications/). Many document formats are ZIP archives with a fixed internal layout — [EPUB](https://www.w3.org/TR/epub/), [Office Open XML](https://www.iso.org/standard/61796.html) (DOCX/XLSX/PPTX), [OpenDocument](https://www.iso.org/standard/66376.html) (ODT/ODS/ODP) and [OpenXPS](https://www.ecma-international.org/publications-and-standards/standards/ecma-376/) — and all embed a C2PA Manifest Store through this single transport.

The Manifest Store is stored as a dedicated ZIP entry at a fixed location:

| Property | Value |
|---|---|
| Path | `META-INF/content_credential.c2pa` |
| Compression | Stored (method `0`, uncompressed) |
| Encryption | None |
| General-purpose bit flag | `0` |
| Media type | As recommended for external manifests |

Embedding appends the manifest entry before the central directory, so existing entries keep their byte offsets; the central directory and end-of-central-directory record are then rebuilt. All parsing is bounds-checked against untrusted input, and ZIP64 archives are rejected (fail-closed) rather than mis-parsed.

Zero dependencies.

## Quick Start

```toml
[dependencies]
c2pa-zip = "0.1"
```

### Embed a manifest

```rust
use c2pa_zip::embed_manifest;

let doc: &[u8] = /* .epub / .docx / .odt / .oxps bytes */;
let manifest: &[u8] = /* C2PA Manifest Store bytes */;

// Insert (or replace) the manifest entry; existing entries stay byte-stable.
let signed = embed_manifest(doc, manifest).unwrap();
```

### Read a manifest

```rust
use c2pa_zip::read_manifest;

// Some(bytes) when a manifest is present, None when the archive has none.
let manifest = read_manifest(&signed).unwrap();
```

### Remove a manifest

```rust
use c2pa_zip::remove_manifest;

let stripped = remove_manifest(&signed).unwrap();
```

### Verify structurally

```rust
use c2pa_zip::verify;

let report = verify(&signed).unwrap();
// report.has_manifest, report.manifest_len, report.is_valid_zip
```

## Design

- The Manifest Store is a single stored (uncompressed) ZIP entry at `META-INF/content_credential.c2pa`
- `embed_manifest` replaces any existing manifest entry, so the archive always carries at most one; when none is present the existing entries keep their exact byte offsets
- `remove_manifest` rebuilds the archive without the manifest entry, recomputing local-header offsets and the central directory
- `verify` reports transport-level structure only: whether the archive parses and whether a manifest is present
- ZIP64 archives (identified by sentinel values in the EOCD or central directory) are rejected; a trailing ZIP comment is tolerated when locating the EOCD

## Scope

This crate is the ZIP transport only: it reads, writes, and removes a C2PA Manifest Store stored as a ZIP entry. Manifest construction, signing, and hard/soft binding (the collection-data-hash) are out of scope; use the [c2pa SDK](https://crates.io/crates/c2pa) for those. `verify` performs structural checks (presence + parseability), **not** hard-binding validation.

## Related Crates

| Crate | Description |
|---|---|
| [c2pa-fonts](https://crates.io/crates/c2pa-fonts) | OpenType/TrueType (SFNT) font embedding |
| [c2pa-warc](https://crates.io/crates/c2pa-warc) | WARC web archive embedding (ISO 28500) |
| [c2pa-structured-text](https://crates.io/crates/c2pa-structured-text) | Structured text embedding via ASCII armour delimiters |
| [c2pa-text-binding](https://crates.io/crates/c2pa-text-binding) | Soft binding and content fingerprinting for text assets |
| [c2pa-rs](https://crates.io/crates/c2pa) | Official C2PA SDK |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.

Built by [WritersLogic](https://writerslogic.com)
