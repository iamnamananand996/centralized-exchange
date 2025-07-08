use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct PaginationQuery {
    #[serde(deserialize_with = "deserialize_optional_u64")]
    pub page: Option<u64>,
    #[serde(deserialize_with = "deserialize_optional_u64")]
    pub limit: Option<u64>,
}

fn deserialize_optional_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    
    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt {
        Some(s) => s.parse::<u64>().map(Some).map_err(Error::custom),
        None => Ok(None),
    }
}

#[derive(Serialize)]
pub struct PaginationInfo {
    pub current_page: u64,
    pub total_pages: u64,
    pub total_items: u64,
    pub items_per_page: u64,
}

#[derive(Serialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub pagination: PaginationInfo,
}

impl PaginationQuery {
    pub fn get_page(&self) -> u64 {
        self.page.unwrap_or(1)
    }

    pub fn get_limit(&self) -> u64 {
        self.limit.unwrap_or(10).min(100) // Max 100 items per page
    }

    pub fn get_offset(&self) -> u64 {
        let page = self.get_page();
        let limit = self.get_limit();
        if page > 0 {
            (page - 1) * limit
        } else {
            0
        }
    }
}

impl PaginationInfo {
    pub fn new(current_page: u64, total_items: u64, items_per_page: u64) -> Self {
        let total_pages = if total_items == 0 {
            1
        } else {
            (total_items as f64 / items_per_page as f64).ceil() as u64
        };

        Self {
            current_page,
            total_pages,
            total_items,
            items_per_page,
        }
    }
}

impl<T> PaginatedResponse<T> {
    pub fn new(data: Vec<T>, pagination: PaginationInfo) -> Self {
        Self { data, pagination }
    }
} 