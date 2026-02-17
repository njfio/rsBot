#![no_main]

use libfuzzer_sys::fuzz_target;
use tau_gateway::{
    best_effort_gateway_ws_request_id, build_gateway_ws_error_frame,
    classify_gateway_ws_parse_error, parse_gateway_ws_request_frame,
};

fuzz_target!(|data: &[u8]| {
    let raw = String::from_utf8_lossy(data);
    let maybe_request_id = best_effort_gateway_ws_request_id(&raw);

    match parse_gateway_ws_request_frame(&raw) {
        Ok(frame) => {
            assert!(!frame.request_id.trim().is_empty());
            assert!(!frame.payload.contains_key(""));
        }
        Err(error) => {
            let code = classify_gateway_ws_parse_error(&error.to_string());
            assert!(!code.trim().is_empty());
            let request_id = maybe_request_id.as_deref().unwrap_or("fuzz-request");
            let error_frame = build_gateway_ws_error_frame(request_id, code, &error.to_string());
            assert_eq!(error_frame.kind, "error");
            assert!(!error_frame.request_id.trim().is_empty());
        }
    }
});
