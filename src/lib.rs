// Copyright 2026 WritersLogic. All rights reserved.
// Licensed under the Apache License, Version 2.0 or the MIT license,
// at your option.

//! C2PA Manifest Store embedding and reading for ZIP-based (OCF-style) documents.
//!
//! Implements the ZIP embedding method from the C2PA Technical Specification,
//! which stores a C2PA Manifest Store as a dedicated entry inside the ZIP
//! archive at [`ZIP_MANIFEST_PATH`] (`META-INF/content_credential.c2pa`). Many
//! document formats are ZIP archives with a fixed internal layout — EPUB,
//! Office Open XML (DOCX/XLSX/PPTX), OpenDocument (ODT/ODS/ODP) and OpenXPS —
//! and all embed through this single transport.
//!
//! Per the specification, the manifest entry is **stored** (compression method
//! 0), not encrypted, and its general-purpose bit flag is `0`. Embedding
//! appends the entry before the central directory so existing entries keep
//! their byte offsets, then rebuilds the central directory and end-of-central-
//! directory record.
//!
//! Manifest construction, signing, and hard/soft binding (the collection-data-
//! hash) are out of scope; use the official [`c2pa`](https://crates.io/crates/c2pa)
//! SDK for those.

mod error;
mod reader;
mod verify;
mod writer;
mod zip;

pub use error::Error;
pub use reader::read_manifest;
pub use verify::{verify, Compliance};
pub use writer::{embed_manifest, remove_manifest};
pub use zip::ZIP_MANIFEST_PATH;
