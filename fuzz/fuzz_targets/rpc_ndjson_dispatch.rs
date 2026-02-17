#![no_main]

use libfuzzer_sys::fuzz_target;
use tau_runtime::dispatch_rpc_ndjson_input;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);
    let report = dispatch_rpc_ndjson_input(&input);
    assert!(report.error_count <= report.processed_lines);
    assert!(report.responses.len() <= report.processed_lines);
});
