// @author kongweiguang

//! Streaming fixture generator for release benchmarks; never keeps the target file in memory.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let format = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| usage_error("missing format"))?;
    let target_bytes = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| usage_error("missing target byte count"))?
        .parse::<u64>()?;
    let path = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| usage_error("missing output path"))?;
    if target_bytes == 0 {
        return Err(usage_error("target byte count must be positive").into());
    }

    let file = File::create(path)?;
    let mut writer = BufWriter::with_capacity(8 * 1024 * 1024, file);
    match format.as_str() {
        "text" => generate_repeated_rows(
            &mut writer,
            target_bytes,
            b"log line 0000000000 payload payload payload\n",
        )?,
        "csv" => {
            writer.write_all(b"id,name,note\n")?;
            generate_repeated_rows(
                &mut writer,
                target_bytes.saturating_sub(13),
                b"0000000000,Alice,repeatable benchmark payload\n",
            )?;
        }
        "jsonl" => generate_repeated_rows(
            &mut writer,
            target_bytes,
            b"{\"id\":0,\"name\":\"Alice\",\"active\":true}\n",
        )?,
        "markdown" => {
            let header = b"| id | name | note |\n| ---: | --- | --- |\n";
            writer.write_all(header)?;
            generate_repeated_rows(
                &mut writer,
                target_bytes.saturating_sub(header.len() as u64),
                b"| 0000000000 | Alice | repeatable benchmark payload |\n",
            )?;
        }
        "mixed-markdown" => generate_mixed_markdown(&mut writer, target_bytes)?,
        "json" => generate_json_array(&mut writer, target_bytes)?,
        _ => {
            return Err(usage_error(
                "format must be text, csv, jsonl, json, markdown, or mixed-markdown",
            )
            .into());
        }
    }
    writer.flush()?;
    Ok(())
}

fn generate_mixed_markdown(writer: &mut impl Write, target_bytes: u64) -> std::io::Result<()> {
    let long_code = format!("```text\n{} NEEDLE-tail\n```\n\n", "x".repeat(64 * 1024));
    let chunks = [
        "# 生产报告 😀\n\n中英文段落 payload，包含 emoji 👩‍💻、组合音标 e\u{301} 与かな。\n\n",
        "- 一级列表\n  - 二级列表\n    - 三级列表\n      - 四级 payload\n\n",
        "| 编号 | 姓名 | 说明 |\n| ---: | :--- | --- |\n| 42 | 小明 😀 | payload with `code` |\n\n",
        "![本地图片](assets/示例 image.png)\n\n",
        long_code.as_str(),
    ];
    let mut written = 0u64;
    while written < target_bytes {
        for chunk in &chunks {
            let remaining = target_bytes - written;
            if remaining == 0 {
                break;
            }
            let bytes = chunk.as_bytes();
            if bytes.len() as u64 <= remaining {
                writer.write_all(bytes)?;
                written += bytes.len() as u64;
                continue;
            }
            let mut take = remaining as usize;
            while take > 0 && !chunk.is_char_boundary(take) {
                take -= 1;
            }
            writer.write_all(&bytes[..take])?;
            written += take as u64;
            while written < target_bytes {
                writer.write_all(b" ")?;
                written += 1;
            }
            break;
        }
    }
    Ok(())
}

fn generate_repeated_rows(
    writer: &mut impl Write,
    target_bytes: u64,
    row: &[u8],
) -> std::io::Result<()> {
    let mut written = 0u64;
    while written < target_bytes {
        let remaining = target_bytes - written;
        let take = usize::try_from(remaining.min(row.len() as u64)).unwrap_or(row.len());
        writer.write_all(&row[..take])?;
        written += take as u64;
    }
    Ok(())
}

fn generate_json_array(writer: &mut impl Write, target_bytes: u64) -> std::io::Result<()> {
    const ITEM: &[u8] = b"{\"id\":0,\"name\":\"Alice\",\"active\":true}";
    writer.write_all(b"[")?;
    let mut written = 1u64;
    let mut first = true;
    while written + 1 < target_bytes {
        let prefix = if first {
            b"".as_slice()
        } else {
            b",".as_slice()
        };
        if written + prefix.len() as u64 + ITEM.len() as u64 + 1 > target_bytes {
            break;
        }
        writer.write_all(prefix)?;
        writer.write_all(ITEM)?;
        written += prefix.len() as u64 + ITEM.len() as u64;
        first = false;
    }
    writer.write_all(b"]")
}

fn usage_error(message: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!(
            "{message}; usage: generate_fixture <text|csv|jsonl|json|markdown|mixed-markdown> <bytes> <output>"
        ),
    )
}
