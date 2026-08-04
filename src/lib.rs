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
//! # Binding inputs
//!
//! A ZIP asset is bound with a `c2pa.hash.collection.data` assertion carrying
//! one entry per member plus the additional `zip_central_directory_hash` field,
//! which covers the archive's own directory so that an entry added after
//! signing cannot leave the manifest valid.
//!
//! [`collection_members`] and [`central_directory_range`] supply the byte ranges
//! the specification says to hash. They deliberately do not hash them: locating
//! those ranges is ZIP parsing, which is this crate's job, while choosing and
//! running a digest is the caller's, and keeping that split is what lets this
//! crate stay dependency-free.
//!
//! Manifest construction and signing remain out of scope; use the official
//! [`c2pa`](https://crates.io/crates/c2pa) SDK for those.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
mod binding;
mod error;
mod reader;
mod verify;
mod writer;
mod zip;

#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(all(feature = "python", not(target_arch = "wasm32")))]
mod python;

pub use binding::{central_directory_range, collection_members, Member};
pub use error::Error;
pub use reader::read_manifest;
pub use verify::{verify, Compliance};
pub use writer::{embed_manifest, remove_manifest};
pub use zip::ZIP_MANIFEST_PATH;
