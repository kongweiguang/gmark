// @author kongweiguang

//! Release-mode stage benchmark for a caller-supplied text file.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use gmark_paged_document::{
    DelimitedIndex, DelimitedIndexOptions, DocumentFormat, FileSource, JsonIndex, JsonIndexOptions,
    LineIndex, MarkdownTableIndex, PagedDocument, PieceDocument, ProbeOptions, SearchCancellation,
    SearchOptions, ViewportRequest, probe_file, search_file_source,
    validate_json_lines_cancellable,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "usage: large_file_probe <text-file>",
            )
        })?;

    let probe_started = Instant::now();
    let probe = probe_file(&path, ProbeOptions::default())?;
    let probe_elapsed = probe_started.elapsed();
    let source = FileSource::open(&path)?;

    let first_viewport_started = Instant::now();
    let provisional_rows = probe.estimated_lines.min(120);
    for line in 0..provisional_rows {
        let offset =
            ((probe.len as u128 * line as u128) / probe.estimated_lines.max(1) as u128) as u64;
        let end = offset.saturating_add(64 * 1024).min(probe.len);
        let _ = source.read_range(offset, end)?;
    }
    let first_viewport_elapsed = first_viewport_started.elapsed();

    let query = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "payload".to_owned());
    let first_search_started = Instant::now();
    let first_search = search_file_source(
        &source,
        &query,
        SearchOptions {
            result_limit: 1,
            ..SearchOptions::default()
        },
        &SearchCancellation::default(),
    )?;
    let first_search_elapsed = first_search_started.elapsed();

    let full_search_started = Instant::now();
    let full_search = search_file_source(
        &source,
        &query,
        SearchOptions::default(),
        &SearchCancellation::default(),
    )?;
    let full_search_elapsed = full_search_started.elapsed();

    let lines_started = Instant::now();
    let lines = LineIndex::build(&source)?;
    let lines_elapsed = lines_started.elapsed();

    let adapter = PagedDocument::new(PieceDocument::open(source.clone(), lines.clone())?);
    let exact_viewport_started = Instant::now();
    let exact_viewport = adapter.read_viewport(&ViewportRequest::bounded(0, 80, 40, 0, 1))?;
    let exact_viewport_elapsed = exact_viewport_started.elapsed();
    let mut random_viewport_times = Vec::with_capacity(101);
    let last_line = lines.line_count().saturating_sub(1);
    for sample in 0..=100u64 {
        let line = (last_line as u128 * sample as u128 / 100) as u64;
        let started = Instant::now();
        let _ = adapter.read_viewport(&ViewportRequest::bounded(line, 80, 40, 0, sample + 2))?;
        random_viewport_times.push(started.elapsed());
    }
    random_viewport_times.sort_unstable();
    let random_viewport_p95 = random_viewport_times[95];

    let structure_started = Instant::now();
    let structure_summary = match probe.format {
        DocumentFormat::Markdown => {
            let tables = MarkdownTableIndex::detect_all(&source, lines.clone())?;
            format!("markdown_tables={}", tables.len())
        }
        DocumentFormat::Delimited { delimiter } => {
            let index = DelimitedIndex::build(
                &source,
                DelimitedIndexOptions {
                    delimiter,
                    ..DelimitedIndexOptions::default()
                },
            )?;
            format!("delimited_records={}", index.record_count())
        }
        DocumentFormat::Json => {
            let index = JsonIndex::build(&source, JsonIndexOptions::default())?;
            format!("json_root_items={}", index.item_count())
        }
        DocumentFormat::JsonLines => {
            validate_json_lines_cancellable(&source, &lines, &SearchCancellation::default())?;
            format!("json_lines={}", lines.line_count())
        }
        DocumentFormat::PlainText => "plain_text=true".to_owned(),
    };
    let structure_elapsed = structure_started.elapsed();

    println!("bytes={}", probe.len);
    println!("estimated_lines={}", probe.estimated_lines);
    println!("actual_lines={}", lines.line_count());
    let line_storage = lines.storage_stats();
    println!("line_index_disk_backed={}", line_storage.disk_backed);
    println!("line_index_pages={}", line_storage.page_count);
    println!("line_index_resident_pages={}", line_storage.resident_pages);
    println!(
        "line_index_resident_encoded_bytes={}",
        line_storage.resident_encoded_bytes
    );
    println!(
        "line_index_resident_decoded_bytes={}",
        line_storage.resident_decoded_bytes
    );
    println!(
        "line_index_max_resident_pages={}",
        line_storage.max_resident_pages
    );
    println!("probe_ms={:.3}", milliseconds(probe_elapsed));
    println!(
        "provisional_viewport_ms={:.3}",
        milliseconds(first_viewport_elapsed)
    );
    println!("provisional_viewport_rows={provisional_rows}");
    println!("search_first_ms={:.3}", milliseconds(first_search_elapsed));
    println!("search_first_found={}", !first_search.is_empty());
    println!(
        "search_to_limit_ms={:.3}",
        milliseconds(full_search_elapsed)
    );
    println!("search_results={}", full_search.len());
    println!("line_index_ms={:.3}", milliseconds(lines_elapsed));
    println!(
        "line_index_mib_per_s={:.1}",
        throughput_mib_per_second(probe.len, lines_elapsed)
    );
    println!("structure_ms={:.3}", milliseconds(structure_elapsed));
    println!(
        "exact_viewport_ms={:.3}",
        milliseconds(exact_viewport_elapsed)
    );
    println!(
        "random_viewport_p95_ms={:.3}",
        milliseconds(random_viewport_p95)
    );
    println!("exact_viewport_rows={}", exact_viewport.lines.len());
    println!(
        "structure_mib_per_s={:.1}",
        throughput_mib_per_second(probe.len, structure_elapsed)
    );
    println!("{structure_summary}");
    Ok(())
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn throughput_mib_per_second(bytes: u64, duration: Duration) -> f64 {
    let seconds = duration.as_secs_f64();
    if seconds == 0.0 {
        return 0.0;
    }
    bytes as f64 / (1024.0 * 1024.0) / seconds
}
