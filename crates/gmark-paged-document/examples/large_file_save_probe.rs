// @author kongweiguang

//! Release-mode streaming atomic-save benchmark for a caller-supplied large file.

use std::path::PathBuf;
use std::time::Instant;

use gmark_paged_document::{FileSource, LineIndex, PieceDocument, SearchCancellation};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let source_path = arguments.next().map(PathBuf::from).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "usage: large_file_save_probe <source> <destination>",
        )
    })?;
    let destination = arguments.next().map(PathBuf::from).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "usage: large_file_save_probe <source> <destination>",
        )
    })?;
    if source_path == destination || destination.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "destination must be a new path different from the source",
        )
        .into());
    }

    let source = FileSource::open(&source_path)?;
    let bytes = source.identity()?.len;
    let index_started = Instant::now();
    let index = LineIndex::build(&source)?;
    let index_elapsed = index_started.elapsed();
    let mut document = PieceDocument::open(source, index)?;
    let save_started = Instant::now();
    document.save_atomic_cancellable(&destination, &SearchCancellation::default())?;
    let save_elapsed = save_started.elapsed();

    println!("bytes={bytes}");
    println!("index_seconds={:.6}", index_elapsed.as_secs_f64());
    println!("save_seconds={:.6}", save_elapsed.as_secs_f64());
    println!(
        "save_mib_per_s={:.3}",
        bytes as f64 / (1024.0 * 1024.0) / save_elapsed.as_secs_f64()
    );
    println!("saved_bytes={}", std::fs::metadata(destination)?.len());
    Ok(())
}
