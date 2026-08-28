//! WebAssembly bindings, built only for the `wasm32` target and published to
//! npm as `c2pa-zip`.
//!
//! Archives map to and from `Uint8Array`. A document carrying no manifest
//! returns `null` from [`readManifest`](fn.read_manifest.html) rather than
//! throwing, because absence of provenance is not an error.

use wasm_bindgen::prelude::*;

fn js_err(e: crate::Error) -> JsError {
    JsError::new(&e.to_string())
}

/// Embed a C2PA Manifest Store into a ZIP-based document.
#[wasm_bindgen(js_name = embedManifest)]
pub fn embed_manifest(zip: &[u8], store: &[u8]) -> Result<Vec<u8>, JsError> {
    crate::embed_manifest(zip, store).map_err(js_err)
}

/// Read the embedded C2PA Manifest Store, or `null` when there is none.
#[wasm_bindgen(js_name = readManifest)]
pub fn read_manifest(zip: &[u8]) -> Result<Option<Vec<u8>>, JsError> {
    crate::read_manifest(zip).map_err(js_err)
}

/// Remove the C2PA Manifest Store from a ZIP-based document.
#[wasm_bindgen(js_name = removeManifest)]
pub fn remove_manifest(zip: &[u8]) -> Result<Vec<u8>, JsError> {
    crate::remove_manifest(zip).map_err(js_err)
}

/// The members covered by the collection hash, as objects with `name`,
/// `start`, and `length`.
#[wasm_bindgen(js_name = collectionMembers)]
pub fn collection_members(zip: &[u8]) -> Result<Vec<JsValue>, JsError> {
    Ok(crate::collection_members(zip)
        .map_err(js_err)?
        .into_iter()
        .map(|m| {
            let out = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&out, &"name".into(), &m.name.as_str().into());
            let _ = js_sys::Reflect::set(&out, &"start".into(), &(m.content.start as u32).into());
            let _ = js_sys::Reflect::set(&out, &"length".into(), &(m.content.len() as u32).into());
            out.into()
        })
        .collect())
}

/// The `{ start, length }` byte range of the ZIP central directory.
#[wasm_bindgen(js_name = centralDirectoryRange)]
pub fn central_directory_range(zip: &[u8]) -> Result<JsValue, JsError> {
    let r = crate::central_directory_range(zip).map_err(js_err)?;
    let out = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&out, &"start".into(), &(r.start as u32).into());
    let _ = js_sys::Reflect::set(&out, &"length".into(), &(r.len() as u32).into());
    Ok(out.into())
}

/// Ordered `{ start, length }` ranges to concatenate for the central-directory
/// hash, excluding the manifest entry's CRC-32.
#[wasm_bindgen(js_name = centralDirectoryRanges)]
pub fn central_directory_ranges(zip: &[u8]) -> Result<Vec<JsValue>, JsError> {
    Ok(crate::central_directory_ranges(zip)
        .map_err(js_err)?
        .into_iter()
        .map(|range| {
            let out = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&out, &"start".into(), &(range.start as u32).into());
            let _ = js_sys::Reflect::set(&out, &"length".into(), &(range.len() as u32).into());
            out.into()
        })
        .collect())
}

/// Structurally verify the embedding, as `{ hasManifest, manifestLen,
/// isValidZip }`. Transport-level only: no signature or collection hash is
/// checked.
#[wasm_bindgen(js_name = verify)]
pub fn verify(zip: &[u8]) -> Result<JsValue, JsError> {
    let c = crate::verify(zip).map_err(js_err)?;
    let out = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&out, &"hasManifest".into(), &c.has_manifest.into());
    let _ = js_sys::Reflect::set(&out, &"manifestLen".into(), &(c.manifest_len as u32).into());
    let _ = js_sys::Reflect::set(&out, &"isValidZip".into(), &c.is_valid_zip.into());
    Ok(out.into())
}

/// The path the Manifest Store is stored at inside the container.
#[wasm_bindgen(js_name = manifestPath)]
pub fn manifest_path() -> String {
    crate::ZIP_MANIFEST_PATH.to_string()
}
