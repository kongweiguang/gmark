// @author kongweiguang

use crate::{DocumentViewId, ViewDescriptor};

pub const MIB: u64 = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentFormat {
    PlainText,
    Markdown,
    Json,
    JsonLines,
    Delimited { delimiter: u8 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8 { bom: bool },
    Utf16Le,
    Utf16Be,
    Legacy(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentProfile {
    pub len: u64,
    pub format: DocumentFormat,
    pub encoding: TextEncoding,
    pub estimated_lines: u64,
    pub estimated_structural_units: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadingLimits {
    pub max_resident_bytes: u64,
}

impl LoadingLimits {
    /// Resident 会话沿用打开时冻结的阈值；设置变化只影响下次打开。
    pub fn exceeded_reason(self, profile: &DocumentProfile) -> Option<OpenReason> {
        if profile.len > self.max_resident_bytes {
            Some(OpenReason::ByteLimitExceeded)
        } else {
            None
        }
    }
}

pub const DEFAULT_LOADING_LIMITS: LoadingLimits = LoadingLimits {
    max_resident_bytes: 16 * MIB,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoadingPolicy {
    pub max_resident_bytes: Option<u64>,
    pub force_safe_source: bool,
}

impl LoadingPolicy {
    pub fn effective_limits(self) -> LoadingLimits {
        LoadingLimits {
            max_resident_bytes: self
                .max_resident_bytes
                .unwrap_or(DEFAULT_LOADING_LIMITS.max_resident_bytes),
        }
    }

    /// 格式只决定视图；只有文件体积越界时才切换到 Paged Source。
    pub fn resolve(self, profile: &DocumentProfile) -> OpenPlan {
        let limits = self.effective_limits();
        let reason = if self.force_safe_source {
            Some(OpenReason::ForcedSafeSource)
        } else {
            limits.exceeded_reason(profile)
        };

        if let Some(reason) = reason {
            return OpenPlan {
                backend: DocumentBackendKind::Paged,
                initial_view: DocumentViewId::source(),
                allowed_views: vec![ViewDescriptor::source()],
                reason,
                limits,
            };
        }

        let allowed_views = ViewDescriptor::regular_views_for(&profile.format);
        let initial_view = match profile.format {
            DocumentFormat::Markdown => DocumentViewId::markdown_live(),
            DocumentFormat::Json => DocumentViewId::json_graph(),
            DocumentFormat::JsonLines => DocumentViewId::json_structure(),
            DocumentFormat::Delimited { .. } => DocumentViewId::delimited_table(),
            DocumentFormat::PlainText => DocumentViewId::source(),
        };
        OpenPlan {
            backend: DocumentBackendKind::Resident,
            initial_view,
            allowed_views,
            reason: OpenReason::WithinResidentLimits,
            limits,
        }
    }
}

/// 打开策略的领域名称；保留 `LoadingPolicy` 作为设置层语义，两者是同一份不可变契约。
pub type OpenPolicy = LoadingPolicy;

/// 纯打开决策入口。Probe、设置与 UI 都只能把画像和策略交给此 resolver，不能自行分支。
#[derive(Clone, Copy, Debug, Default)]
pub struct OpenPolicyResolver;

impl OpenPolicyResolver {
    pub fn resolve(self, policy: OpenPolicy, profile: &DocumentProfile) -> OpenPlan {
        policy.resolve(profile)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentBackendKind {
    Resident,
    Paged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenReason {
    WithinResidentLimits,
    ForcedSafeSource,
    ByteLimitExceeded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenPlan {
    pub backend: DocumentBackendKind,
    pub initial_view: DocumentViewId,
    pub allowed_views: Vec<ViewDescriptor>,
    pub reason: OpenReason,
    /// 打开时解析出的有效阈值。会话必须冻结该值，不能被后续设置变更重解释。
    pub limits: LoadingLimits,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(format: DocumentFormat) -> DocumentProfile {
        DocumentProfile {
            len: DEFAULT_LOADING_LIMITS.max_resident_bytes,
            format,
            encoding: TextEncoding::Utf8 { bom: false },
            estimated_lines: u64::MAX,
            estimated_structural_units: u64::MAX,
        }
    }

    #[test]
    fn only_byte_overflow_uses_paged_source() {
        let policy = LoadingPolicy::default();
        let exact = profile(DocumentFormat::Json);
        assert_eq!(
            policy.resolve(&exact).backend,
            DocumentBackendKind::Resident
        );

        let plan = policy.resolve(&DocumentProfile {
            len: exact.len + 1,
            ..exact
        });
        assert_eq!(plan.backend, DocumentBackendKind::Paged);
        assert_eq!(plan.initial_view, DocumentViewId::source());
        assert_eq!(plan.allowed_views, vec![ViewDescriptor::source()]);
    }

    #[test]
    fn regular_formats_select_their_own_default_views() {
        for (format, expected) in [
            (DocumentFormat::Markdown, DocumentViewId::markdown_live()),
            (DocumentFormat::Json, DocumentViewId::json_graph()),
            (DocumentFormat::JsonLines, DocumentViewId::json_structure()),
            (
                DocumentFormat::Delimited { delimiter: b',' },
                DocumentViewId::delimited_table(),
            ),
            (DocumentFormat::PlainText, DocumentViewId::source()),
        ] {
            let plan = LoadingPolicy::default().resolve(&profile(format));
            assert_eq!(plan.backend, DocumentBackendKind::Resident);
            assert_eq!(plan.initial_view, expected);
        }
    }

    #[test]
    fn safe_source_overrides_regular_profile_without_changing_limits() {
        let policy = LoadingPolicy {
            force_safe_source: true,
            ..LoadingPolicy::default()
        };
        let plan = OpenPolicyResolver.resolve(
            policy,
            &DocumentProfile {
                len: 1,
                ..profile(DocumentFormat::Markdown)
            },
        );
        assert_eq!(plan.backend, DocumentBackendKind::Paged);
        assert_eq!(plan.reason, OpenReason::ForcedSafeSource);
    }
}
