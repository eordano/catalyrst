use serde::{Deserialize, Serialize};

use crate::ports::db::DbImage;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    pub id: String,
    pub url: String,
    pub thumbnail_url: String,
    pub is_public: bool,
    pub metadata: Metadata,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GalleryImage {
    pub id: String,
    pub url: String,
    pub thumbnail_url: String,
    pub is_public: bool,
    pub date_time: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GalleryImageWithPlace {
    pub id: String,
    pub url: String,
    pub thumbnail_url: String,
    pub is_public: bool,
    pub date_time: String,
    pub place_id: String,
}

#[derive(Deserialize, Serialize, Debug, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub user_name: String,
    pub user_address: String,
    pub date_time: String,
    pub realm: String,
    pub scene: Scene,
    pub visible_people: Vec<User>,
    pub place_id: String,
}

#[derive(Deserialize, Serialize, Debug, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub name: String,
    pub location: Location,
}

#[derive(Deserialize, Serialize, Debug, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub x: String,
    pub y: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub user_name: String,
    pub user_address: String,
    pub wearables: Vec<String>,
    #[serde(default)]
    pub is_guest: bool,
    #[serde(default)]
    pub is_emoting: Option<bool>,
}

impl From<DbImage> for Image {
    fn from(value: DbImage) -> Self {
        Self {
            id: value.id.to_string(),
            url: value.url,
            thumbnail_url: value.thumbnail_url,
            is_public: value.is_public,
            metadata: value.metadata.0,
        }
    }
}

impl From<DbImage> for GalleryImage {
    fn from(value: DbImage) -> Self {
        Self {
            id: value.id.to_string(),
            url: value.url,
            thumbnail_url: value.thumbnail_url,
            is_public: value.is_public,
            date_time: value.metadata.0.date_time,
        }
    }
}

impl From<DbImage> for GalleryImageWithPlace {
    fn from(value: DbImage) -> Self {
        Self {
            id: value.id.to_string(),
            url: value.url,
            thumbnail_url: value.thumbnail_url,
            is_public: value.is_public,
            date_time: value.metadata.0.date_time,
            place_id: value.metadata.0.place_id,
        }
    }
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UserDataResponse {
    pub current_images: u64,
    pub max_images: u64,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlaceDataResponse {
    pub max_images: u64,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UploadResponse {
    pub image: Image,
    #[serde(flatten)]
    pub user_data: UserDataResponse,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetImagesResponse {
    pub images: Vec<Image>,
    #[serde(flatten)]
    pub user_data: UserDataResponse,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetGalleryImagesResponse {
    pub images: Vec<GalleryImage>,
    #[serde(flatten)]
    pub user_data: UserDataResponse,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetPlaceImagesResponse {
    pub images: Vec<GalleryImage>,
    #[serde(flatten)]
    pub place_data: PlaceDataResponse,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetMultiplePlacesImagesResponse {
    pub images: Vec<GalleryImageWithPlace>,
    #[serde(flatten)]
    pub place_data: PlaceDataResponse,
}

#[derive(Deserialize, Debug)]
pub struct UpdateVisibility {
    pub is_public: bool,
}

/// Body for the moderator review PATCH. `review_status` must be one of
/// `ok` | `flagged` | `rejected`.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReview {
    pub review_status: String,
}

impl UpdateReview {
    pub fn is_valid(&self) -> bool {
        matches!(self.review_status.as_str(), "ok" | "flagged" | "rejected")
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetMultiplePlacesImagesBody {
    pub places_ids: Vec<String>,
}
