// @author kongweiguang

//! 文档格式、打开策略、事务与视图的后端无关契约。

mod errors;
mod policy;
mod recovery;
mod snapshot;
mod transaction;
mod view;

pub use errors::{OpenError, PersistenceError, ProjectionError};
pub use policy::{
    BALANCED_LIMITS, DocumentBackendKind, DocumentFormat, DocumentProfile, HIGH_PERFORMANCE_LIMITS,
    LOW_MEMORY_LIMITS, LoadingLimits, LoadingPolicy, LoadingPreset, OpenPlan, OpenPolicy,
    OpenPolicyResolver, OpenReason, TextEncoding,
};
pub use recovery::{RecoveryAction, RecoveryBackend, RecoveryRecord};
pub use snapshot::{DocumentSnapshot, SnapshotError};
pub use transaction::{
    DocumentRevision, EditError, SourceAffinity, SourceAnchor, SourceEdit, SourceSelection,
    Transaction,
};
pub use view::{
    DEFAULT_DELIMITED_COLUMN_WINDOW, DEFAULT_DELIMITED_ROW_WINDOW, DelimitedCellProjection,
    DelimitedWindowProjection, DerivedProjectionProvider, DerivedProjectionRequest,
    DerivedProjectionSnapshot, DerivedProjectionStatus, DerivedViewState, DocumentViewId,
    DocumentViewRegistry, DocumentViewState, ProjectionCancellation, SourceLocator,
    SourceViewState, ViewDescriptor, ViewFormat,
};
