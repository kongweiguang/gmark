// @author kongweiguang

//! Compatibility adapter for pure Markdown resource values.
//!
//! Parsing, lexical classification, and canonical serialization live in
//! `gmark-markdown`. This local record keeps the established field-shaped API
//! because the GPUI runtime extends it with a private probe-cache key.

use std::path::Path;

pub use gmark_markdown::{ResourceKind, ResourceLocation, ResourceStatus};

/// Main-package compatibility projection of a pure resource-card value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceRecord {
    pub label: String,
    pub destination: String,
    pub kind: ResourceKind,
    pub explicit_kind: Option<ResourceKind>,
    pub location: ResourceLocation,
    pub source_markdown: Option<String>,
}

impl ResourceRecord {
    pub fn parse(markdown: &str, base_dir: Option<&Path>) -> Option<Self> {
        gmark_markdown::ResourceRecord::parse(markdown, base_dir).map(Into::into)
    }

    pub fn from_parts(
        label: String,
        destination: String,
        explicit_kind: Option<ResourceKind>,
        base_dir: Option<&Path>,
    ) -> Self {
        gmark_markdown::ResourceRecord::from_parts(label, destination, explicit_kind, base_dir)
            .into()
    }

    pub fn source_or_canonical_markdown(&self) -> String {
        self.markdown_value().source_or_canonical_markdown()
    }

    pub fn with_base_dir(&self, base_dir: Option<&Path>) -> Self {
        self.markdown_value().with_base_dir(base_dir).into()
    }

    pub fn to_markdown(&self) -> String {
        self.markdown_value().to_markdown()
    }

    pub fn is_local(&self) -> bool {
        matches!(self.location, ResourceLocation::Local(_))
    }

    pub fn local_path(&self) -> Option<&Path> {
        match &self.location {
            ResourceLocation::Local(path) => Some(path),
            ResourceLocation::Url(_) => None,
        }
    }

    pub fn is_unsafe_url(&self) -> bool {
        self.markdown_value().is_unsafe_url()
    }

    /// Produces the shared, rendering-neutral resource value.
    pub(crate) fn markdown_value(&self) -> gmark_markdown::ResourceRecord {
        self.clone().into()
    }
}

impl From<gmark_markdown::ResourceRecord> for ResourceRecord {
    fn from(value: gmark_markdown::ResourceRecord) -> Self {
        Self {
            label: value.label,
            destination: value.destination,
            kind: value.kind,
            explicit_kind: value.explicit_kind,
            location: value.location,
            source_markdown: value.source_markdown,
        }
    }
}

impl From<ResourceRecord> for gmark_markdown::ResourceRecord {
    fn from(value: ResourceRecord) -> Self {
        Self {
            label: value.label,
            destination: value.destination,
            kind: value.kind,
            explicit_kind: value.explicit_kind,
            location: value.location,
            source_markdown: value.source_markdown,
        }
    }
}

#[cfg(test)]
pub(crate) use gmark_markdown::parse_resource_parts;

#[cfg(test)]
#[path = "../../../tests/unit/components/markdown/resource.rs"]
mod tests;
