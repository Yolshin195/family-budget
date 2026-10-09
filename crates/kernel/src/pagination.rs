//! @managed rust-hex-api — пагинация списков.
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, Deserialize, ToSchema, IntoParams)]
pub struct Pagination {
    #[serde(default)]
    pub page: i64,
    #[serde(default = "default_size")]
    pub size: i64,
}

fn default_size() -> i64 {
    100
}

impl Pagination {
    pub fn offset(&self) -> i64 {
        self.page.max(0) * self.limit()
    }

    pub fn limit(&self) -> i64 {
        self.size.clamp(1, 1000)
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PagedResponse<T> {
    pub content: Vec<T>,
    pub page: PageInfo,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PageInfo {
    pub size: i64,
    pub number: i64,
    #[serde(rename = "totalElements")]
    pub total_elements: i64,
    #[serde(rename = "totalPages")]
    pub total_pages: i64,
}

impl<T> PagedResponse<T> {
    pub fn new(content: Vec<T>, pagination: &Pagination, total_elements: i64) -> Self {
        let size = pagination.limit();
        Self {
            content,
            page: PageInfo {
                size,
                number: pagination.page,
                total_elements,
                total_pages: (total_elements + size - 1) / size,
            },
        }
    }
}
