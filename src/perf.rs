// @author kongweiguang

//! 可选的编辑器性能事件采样。
//!
//! 默认关闭；设置 `GMARK_PERF_TRACE=1` 后向 stderr 输出一行一个 JSON 记录。
//! 这里记录的是 GPUI 可观测的构建/render 边界，不把 render 冒充平台 present。

use std::cell::Cell;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use gmark_document_core::{DocumentBackendKind, DocumentFormat, OpenPlan, OpenReason};

static ENABLED: OnceLock<bool> = OnceLock::new();
static SEQUENCE: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// GPUI 输入与 Entity 事件在同一 UI 线程传递；只保留一轮批量编辑最早的起点。
    static INPUT_MUTATION_STARTED: Cell<Option<Instant>> = const { Cell::new(None) };
}

#[derive(Serialize)]
struct TraceRecord<'a> {
    schema_version: u8,
    sequence: u64,
    unix_time_ms: u128,
    event: &'a str,
    elapsed_us: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    success: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    document_format: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    document_backend: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    open_reason: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<u64>,
}

#[derive(Clone, Copy)]
pub(crate) struct PendingInputTrace {
    started: Instant,
}

pub(crate) fn env_value_enables_trace(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn enabled() -> bool {
    *ENABLED.get_or_init(|| {
        std::env::var("GMARK_PERF_TRACE")
            .ok()
            .is_some_and(|value| env_value_enables_trace(&value))
    })
}

pub(crate) fn start() -> Option<Instant> {
    enabled().then(Instant::now)
}

pub(crate) fn begin_input_mutation() {
    if !enabled() {
        return;
    }
    INPUT_MUTATION_STARTED.with(|started| {
        if started.get().is_none() {
            started.set(Some(Instant::now()));
        }
    });
}

pub(crate) fn take_input_mutation() -> Option<PendingInputTrace> {
    if !enabled() {
        return None;
    }
    INPUT_MUTATION_STARTED.with(|started| {
        started
            .replace(None)
            .map(|started| PendingInputTrace { started })
    })
}

impl PendingInputTrace {
    pub(crate) fn record_dirty_sync(self, source_bytes: usize) {
        emit(
            "input_to_dirty_sync",
            self.started,
            Some(source_bytes),
            None,
            None,
        );
    }

    pub(crate) fn record_next_render(self, source_bytes: usize) {
        emit(
            "input_to_next_render",
            self.started,
            Some(source_bytes),
            None,
            Some("GPUI render boundary; not platform present"),
        );
    }
}

pub(crate) fn emit(
    event: &'static str,
    started: Instant,
    source_bytes: Option<usize>,
    success: Option<bool>,
    detail: Option<&str>,
) {
    if !enabled() {
        return;
    }
    let elapsed_us = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
    let unix_time_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    let record = TraceRecord {
        schema_version: 1,
        sequence: SEQUENCE.fetch_add(1, Ordering::Relaxed),
        unix_time_ms,
        event,
        elapsed_us,
        source_bytes,
        success,
        detail,
        document_format: None,
        document_backend: None,
        open_reason: None,
        value: None,
    };
    emit_record(&record);
}

/// 记录文档生命周期边界。字段只接受封闭领域枚举和数值，禁止正文、路径或错误文本
/// 进入本地诊断；这样即使用户显式开启 trace，也不会泄露文档内容与文件位置。
pub(crate) fn emit_document(
    event: &'static str,
    started: Instant,
    source_bytes: Option<usize>,
    success: Option<bool>,
    format: &DocumentFormat,
    plan: &OpenPlan,
    detail: Option<&'static str>,
) {
    if !enabled() {
        return;
    }
    let record = TraceRecord {
        schema_version: 1,
        sequence: SEQUENCE.fetch_add(1, Ordering::Relaxed),
        unix_time_ms: unix_time_ms(),
        event,
        elapsed_us: started.elapsed().as_micros().min(u64::MAX as u128) as u64,
        source_bytes,
        success,
        detail,
        document_format: Some(document_format_name(format)),
        document_backend: Some(document_backend_name(plan.backend)),
        open_reason: Some(open_reason_name(plan.reason)),
        value: None,
    };
    emit_record(&record);
}

/// 记录有界缓存和取消计数。调用方只在新峰值或离散状态变化时调用，避免每帧刷日志。
pub(crate) fn emit_document_value(
    event: &'static str,
    value: u64,
    format: &DocumentFormat,
    plan: &OpenPlan,
) {
    if !enabled() {
        return;
    }
    let record = TraceRecord {
        schema_version: 1,
        sequence: SEQUENCE.fetch_add(1, Ordering::Relaxed),
        unix_time_ms: unix_time_ms(),
        event,
        elapsed_us: 0,
        source_bytes: None,
        success: None,
        detail: None,
        document_format: Some(document_format_name(format)),
        document_backend: Some(document_backend_name(plan.backend)),
        open_reason: Some(open_reason_name(plan.reason)),
        value: Some(value),
    };
    emit_record(&record);
}

fn unix_time_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

fn document_format_name(format: &DocumentFormat) -> &'static str {
    match format {
        DocumentFormat::PlainText => "plain_text",
        DocumentFormat::Markdown => "markdown",
        DocumentFormat::Json => "json",
        DocumentFormat::JsonLines => "json_lines",
        DocumentFormat::Delimited { delimiter: b'\t' } => "tsv",
        DocumentFormat::Delimited { .. } => "csv",
    }
}

fn document_backend_name(backend: DocumentBackendKind) -> &'static str {
    match backend {
        DocumentBackendKind::Resident => "resident",
        DocumentBackendKind::Paged => "paged",
    }
}

fn open_reason_name(reason: OpenReason) -> &'static str {
    match reason {
        OpenReason::WithinResidentLimits => "within_resident_limits",
        OpenReason::ForcedSafeSource => "forced_safe_source",
        OpenReason::ByteLimitExceeded => "byte_limit_exceeded",
        OpenReason::LineLimitExceeded => "line_limit_exceeded",
        OpenReason::StructuralLimitExceeded => "structural_limit_exceeded",
    }
}

fn emit_record(record: &TraceRecord<'_>) {
    if let Ok(json) = serde_json::to_string(&record) {
        eprintln!("gmark_perf {json}");
    }
}
