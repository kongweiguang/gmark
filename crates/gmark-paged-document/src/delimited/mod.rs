// @author kongweiguang

//! CSV/TSV 稀疏记录索引；只在请求视口时物化字段。

mod index;
mod model;
mod sidecar;
mod source;
mod transform;

pub use model::{
    DelimitedEdit, DelimitedFilterOptions, DelimitedIndex, DelimitedIndexOptions, DelimitedRecord,
    MAX_DELIMITED_RECORD_BYTES,
};
pub use transform::{apply_delimited_column_edit, serialize_delimited_record};

#[cfg(test)]
#[path = "../../tests/unit/delimited.rs"]
mod tests;
