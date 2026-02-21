pub fn prod_panic_path() {
    panic!("prod panic marker");
}

pub fn prod_unsafe_path(buffer: &mut [u8]) {
    if buffer.is_empty() {
        return;
    }
    // Fixture-only unsafe marker for audit classification coverage.
    unsafe {
        let ptr = buffer.as_mut_ptr();
        *ptr = 42;
    }
}

#[cfg(test)]
mod tests {
    pub fn cfg_test_panic_marker() {
        panic!("cfg test panic marker");
    }

    pub fn cfg_test_unsafe_marker(bytes: &mut [u8]) {
        if bytes.is_empty() {
            return;
        }
        // Fixture-only unsafe marker in cfg(test) context.
        unsafe {
            let ptr = bytes.as_mut_ptr();
            *ptr = 9;
        }
    }
}
