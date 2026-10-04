//! `validex memcheck <schema> <data>` — internal helper used by
//! `scripts/profile_memory.py`: runs the same check pipeline in-process and
//! prints the process peak RSS as `peak_rss_bytes <N>` on stdout.
//!
//! Peak memory sources per platform:
//! * Windows: `GetProcessMemoryInfo` (PeakWorkingSetSize) — the only unsafe
//!   block in this crate, an OS-API boundary like the FFI crates.
//! * Linux: `/proc/self/status` `VmHWM` (peak resident set).
//! * macOS: not available in std — the profiling script falls back to
//!   `/usr/bin/time -l` there.

use crate::Args;

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let args = Args::parse(args)?;
    let mut positional = args.positional.iter();
    let schema_path = positional.next().ok_or("memcheck requires <schema>")?;
    let data_path = positional.next().ok_or("memcheck requires <data>")?;

    // Same flow as `check --quiet`, minus reporting.
    let schema_text = std::fs::read_to_string(schema_path)
        .map_err(|e| format!("cannot read schema {schema_path}: {e}"))?;
    let validator = tpt_valid_schema::Validator::new(&schema_text).map_err(|e| e.to_string())?;
    let opts = tpt_valid_core::ValidationOptions::default();
    let lower = data_path.to_ascii_lowercase();
    let failures = if lower.ends_with(".jsonl") || lower.ends_with(".ndjson") {
        // The real streaming path: line-by-line, bounded memory.
        let file =
            std::fs::File::open(data_path).map_err(|e| format!("cannot read {data_path}: {e}"))?;
        let stats = validator
            .validate_jsonl(file, &opts)
            .map_err(|e| e.to_string())?;
        stats.invalid_lines + stats.parse_errors
    } else {
        let text = std::fs::read_to_string(data_path)
            .map_err(|e| format!("cannot read {data_path}: {e}"))?;
        let value = tpt_valid_parser::parse(&text).map_err(|e| e.to_string())?;
        match value {
            serde_json::Value::Array(items) => {
                let outcomes = tpt_valid_core::validate_batch(validator.root(), &items, &opts);
                outcomes.iter().filter(|o| !o.valid).count()
            }
            other => usize::from(!validator.validate(&other).is_valid()),
        }
    };

    let peak = peak_rss_bytes().unwrap_or(0);
    println!("peak_rss_bytes {peak}");
    println!("failed_records {failures}");
    Ok(())
}

/// The process peak RSS in bytes (`None` when unsupported).
fn peak_rss_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmHWM:") {
                let kb: u64 = rest.trim().trim_end_matches("kB").trim().parse().ok()?;
                return Some(kb * 1024);
            }
        }
        None
    }

    #[cfg(windows)]
    {
        // SAFETY: `GetCurrentProcess` returns a valid pseudo-handle and
        // `GetProcessMemoryInfo` fills a correctly-sized, zeroed
        // `PROCESS_MEMORY_COUNTERS`. Both are documented Windows API calls.
        windows_peak_working_set()
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    {
        None
    }
}

#[cfg(windows)]
fn windows_peak_working_set() -> Option<u64> {
    use std::os::raw::c_void;

    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "psapi")]
    extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    let mut counters = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };
    // SAFETY: see `peak_rss_bytes`.
    let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    if ok == 0 {
        return None;
    }
    Some(counters.peak_working_set_size as u64)
}
