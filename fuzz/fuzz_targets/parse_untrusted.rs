#![no_main]

use c2pa_zip::read_manifest;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = read_manifest(data);
});
