#![no_main]

use libfuzzer_sys::fuzz_target;
use tau_runtime::{dispatch_rpc_raw_with_error_envelope, RPC_FRAME_SCHEMA_VERSION};

fuzz_target!(|data: &[u8]| {
    let raw = String::from_utf8_lossy(data);
    let response = dispatch_rpc_raw_with_error_envelope(&raw);
    assert_eq!(response.schema_version, RPC_FRAME_SCHEMA_VERSION);
    assert!(!response.request_id.trim().is_empty());
    assert!(!response.kind.trim().is_empty());
    assert!(response.payload.is_object());
});
