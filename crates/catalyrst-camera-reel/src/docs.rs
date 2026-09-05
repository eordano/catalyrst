use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

pub async fn openapi_json() -> Response {
    let spec = openapi_spec();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        Json(spec),
    )
        .into_response()
}

pub async fn swagger_ui() -> Html<String> {
    Html(SWAGGER_UI_HTML.to_string())
}

const SWAGGER_UI_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>Camera Reel Service - Swagger UI</title>
<link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist/swagger-ui.css">
</head>
<body>
<div id="swagger-ui"></div>
<script src="https://unpkg.com/swagger-ui-dist/swagger-ui-bundle.js" crossorigin></script>
<script>
window.onload = function() {
  window.ui = SwaggerUIBundle({
    url: "/api/docs/openapi.json",
    dom_id: "#swagger-ui",
    deepLinking: true,
    presets: [SwaggerUIBundle.presets.apis]
  });
};
</script>
</body>
</html>"##;

fn ref_to(name: &str) -> Value {
    json!({ "$ref": format!("#/components/schemas/{name}") })
}

fn openapi_spec() -> Value {
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Camera Reel Service",
            "description": "Camera Reel API",
            "license": { "name": "" },
            "version": env!("CARGO_PKG_VERSION")
        },
        "paths": {
            "/api/images": {
                "post": {
                    "tags": ["images"],
                    "operationId": "upload_image",
                    "requestBody": {
                        "description": "Image file and metadata in JSON format.",
                        "content": {
                            "multipart/form-data": { "schema": ref_to("Upload") }
                        },
                        "required": true
                    },
                    "responses": {
                        "200": { "description": "Uploaded image with its metadata", "content": { "application/json": { "schema": ref_to("UploadResponse") } } },
                        "400": { "description": "Bad Request", "content": { "application/json": { "schema": ref_to("ResponseError") } } },
                        "403": { "description": "Forbidden", "content": { "application/json": { "schema": ref_to("ForbiddenError") } } },
                        "500": { "description": "Internal Server Error", "content": { "application/json": { "schema": ref_to("ResponseError") } } }
                    }
                }
            },
            "/api/images/{image_id}": {
                "get": {
                    "tags": ["images"],
                    "operationId": "get_image",
                    "parameters": [{ "name": "image_id", "in": "path", "required": true, "schema": { "type": "string" } }],
                    "responses": {
                        "200": { "description": "Get image", "content": { "application/json": { "schema": ref_to("Image") } } }
                    }
                },
                "delete": {
                    "tags": ["images"],
                    "operationId": "delete_image",
                    "parameters": [{ "name": "image_id", "in": "path", "required": true, "schema": { "type": "string" } }],
                    "responses": {
                        "200": { "description": "Image deleted", "content": { "application/json": { "schema": ref_to("UserDataResponse") } } },
                        "403": { "description": "Forbidden" },
                        "404": { "description": "Image was not found" },
                        "500": { "description": "Failed to delete image" }
                    }
                }
            },
            "/api/images/{image_id}/metadata": {
                "get": {
                    "tags": ["images"],
                    "operationId": "get_metadata",
                    "parameters": [{ "name": "image_id", "in": "path", "required": true, "schema": { "type": "string" } }],
                    "responses": {
                        "200": { "description": "Get image metadata", "content": { "application/json": { "schema": ref_to("Image") } } },
                        "404": { "description": "Not found" }
                    }
                }
            },
            "/api/images/{id}/visibility": {
                "patch": {
                    "tags": ["images"],
                    "operationId": "update_image_visibility",
                    "requestBody": {
                        "description": "Update image visibility",
                        "content": { "application/json": { "schema": ref_to("UpdateVisibility") } },
                        "required": true
                    },
                    "responses": {
                        "200": { "description": "Image visibility updated successfully" },
                        "403": { "description": "Forbidden" },
                        "404": { "description": "Image was not found" },
                        "500": { "description": "Failed to update image visibility" }
                    }
                }
            },
            "/api/users/{user_address}": {
                "get": {
                    "tags": ["images"],
                    "operationId": "get_user_data",
                    "parameters": [{ "name": "user_address", "in": "path", "required": true, "schema": { "type": "string" } }],
                    "responses": {
                        "200": { "description": "Get user data", "content": { "application/json": { "schema": ref_to("UserDataResponse") } } },
                        "404": { "description": "Not found" }
                    }
                }
            },
            "/api/users/{user_address}/images": {
                "get": {
                    "tags": ["images"],
                    "operationId": "get_user_images",
                    "parameters": [
                        { "name": "offset", "in": "query", "required": false, "schema": { "type": "integer", "format": "int64", "minimum": 0 } },
                        { "name": "limit", "in": "query", "required": false, "schema": { "type": "integer", "format": "int64", "minimum": 0 } },
                        { "name": "compact", "in": "query", "required": false, "schema": { "type": "boolean" } }
                    ],
                    "responses": {
                        "200": { "description": "List images for a given user", "content": { "application/json": { "schema": ref_to("GetImagesResponse") } } },
                        "210": { "description": "List gallery images for a given user if `compact=true` (status code is 200, but was not possible to list multiple responses for one status code)", "content": { "application/json": { "schema": ref_to("GetGalleryImagesResponse") } } },
                        "404": { "description": "Not found" }
                    }
                }
            },
            "/api/places/{place_id}/images": {
                "get": {
                    "tags": ["images"],
                    "operationId": "get_place_images",
                    "parameters": [
                        { "name": "offset", "in": "query", "required": false, "schema": { "type": "integer", "format": "int64", "minimum": 0 } },
                        { "name": "limit", "in": "query", "required": false, "schema": { "type": "integer", "format": "int64", "minimum": 0 } }
                    ],
                    "responses": {
                        "200": { "description": "List images for a given place", "content": { "application/json": { "schema": ref_to("GetPlaceImagesResponse") } } },
                        "404": { "description": "Not found" },
                        "502": { "description": "Failed to resolve world name" }
                    }
                }
            },
            "/api/places/images": {
                "post": {
                    "tags": ["images"],
                    "operationId": "get_multiple_places_images",
                    "parameters": [
                        { "name": "offset", "in": "query", "required": false, "schema": { "type": "integer", "format": "int64", "minimum": 0 } },
                        { "name": "limit", "in": "query", "required": false, "schema": { "type": "integer", "format": "int64", "minimum": 0 } }
                    ],
                    "requestBody": {
                        "description": "Object with a list of places IDs",
                        "content": { "application/json": { "schema": ref_to("GetMultiplePlacesImagesBody") } },
                        "required": true
                    },
                    "responses": {
                        "200": { "description": "List images for multiple places", "content": { "application/json": { "schema": ref_to("GetMultiplePlacesImagesResponse") } } },
                        "400": { "description": "Invalid place IDs format" },
                        "502": { "description": "Failed to resolve world name" }
                    }
                }
            }
        },
        "components": {
            "schemas": {
                "Location": {
                    "type": "object",
                    "required": ["x", "y"],
                    "properties": {
                        "x": { "type": "string" },
                        "y": { "type": "string" }
                    }
                },
                "Scene": {
                    "type": "object",
                    "required": ["name", "location"],
                    "properties": {
                        "name": { "type": "string" },
                        "location": ref_to("Location")
                    }
                },
                "User": {
                    "type": "object",
                    "required": ["userName", "userAddress", "wearables"],
                    "properties": {
                        "userName": { "type": "string" },
                        "userAddress": { "type": "string" },
                        "wearables": { "type": "array", "items": { "type": "string" } },
                        "isGuest": { "type": "boolean" },
                        "isEmoting": { "type": "boolean", "nullable": true }
                    }
                },
                "Metadata": {
                    "type": "object",
                    "required": ["userName", "userAddress", "dateTime", "realm", "scene", "visiblePeople", "placeId"],
                    "properties": {
                        "userName": { "type": "string" },
                        "userAddress": { "type": "string" },
                        "dateTime": { "type": "string" },
                        "realm": { "type": "string" },
                        "scene": ref_to("Scene"),
                        "visiblePeople": { "type": "array", "items": ref_to("User") },
                        "placeId": { "type": "string" }
                    }
                },
                "Image": {
                    "type": "object",
                    "required": ["id", "url", "thumbnailUrl", "isPublic", "metadata"],
                    "properties": {
                        "id": { "type": "string" },
                        "url": { "type": "string" },
                        "thumbnailUrl": { "type": "string" },
                        "isPublic": { "type": "boolean" },
                        "metadata": ref_to("Metadata")
                    }
                },
                "GalleryImage": {
                    "type": "object",
                    "required": ["id", "url", "thumbnailUrl", "isPublic", "dateTime"],
                    "properties": {
                        "id": { "type": "string" },
                        "url": { "type": "string" },
                        "thumbnailUrl": { "type": "string" },
                        "isPublic": { "type": "boolean" },
                        "dateTime": { "type": "string" }
                    }
                },
                "GalleryImageWithPlace": {
                    "type": "object",
                    "required": ["id", "url", "thumbnailUrl", "isPublic", "dateTime", "placeId"],
                    "properties": {
                        "id": { "type": "string" },
                        "url": { "type": "string" },
                        "thumbnailUrl": { "type": "string" },
                        "isPublic": { "type": "boolean" },
                        "dateTime": { "type": "string" },
                        "placeId": { "type": "string" }
                    }
                },
                "Upload": {
                    "type": "object",
                    "required": ["image", "metadata"],
                    "properties": {
                        "image": { "type": "string", "format": "binary" },
                        "metadata": { "type": "string" }
                    }
                },
                "UploadResponse": {
                    "type": "object",
                    "required": ["image", "currentImages", "maxImages"],
                    "properties": {
                        "image": ref_to("Image"),
                        "currentImages": { "type": "integer", "format": "int64", "minimum": 0 },
                        "maxImages": { "type": "integer", "format": "int64", "minimum": 0 }
                    }
                },
                "UpdateVisibility": {
                    "type": "object",
                    "required": ["isPublic"],
                    "properties": {
                        "isPublic": { "type": "boolean" }
                    }
                },
                "UserDataResponse": {
                    "type": "object",
                    "required": ["currentImages", "maxImages"],
                    "properties": {
                        "currentImages": { "type": "integer", "format": "int64", "minimum": 0 },
                        "maxImages": { "type": "integer", "format": "int64", "minimum": 0 }
                    }
                },
                "PlaceDataResponse": {
                    "type": "object",
                    "required": ["maxImages"],
                    "properties": {
                        "maxImages": { "type": "integer", "format": "int64", "minimum": 0 }
                    }
                },
                "GetImagesResponse": {
                    "type": "object",
                    "required": ["images", "currentImages", "maxImages"],
                    "properties": {
                        "images": { "type": "array", "items": ref_to("Image") },
                        "currentImages": { "type": "integer", "format": "int64", "minimum": 0 },
                        "maxImages": { "type": "integer", "format": "int64", "minimum": 0 }
                    }
                },
                "GetGalleryImagesResponse": {
                    "type": "object",
                    "required": ["images", "currentImages", "maxImages"],
                    "properties": {
                        "images": { "type": "array", "items": ref_to("GalleryImage") },
                        "currentImages": { "type": "integer", "format": "int64", "minimum": 0 },
                        "maxImages": { "type": "integer", "format": "int64", "minimum": 0 }
                    }
                },
                "GetPlaceImagesResponse": {
                    "type": "object",
                    "required": ["images", "maxImages"],
                    "properties": {
                        "images": { "type": "array", "items": ref_to("GalleryImage") },
                        "maxImages": { "type": "integer", "format": "int64", "minimum": 0 }
                    }
                },
                "GetMultiplePlacesImagesBody": {
                    "type": "object",
                    "required": ["placesIds"],
                    "properties": {
                        "placesIds": { "type": "array", "items": { "type": "string" } }
                    }
                },
                "GetMultiplePlacesImagesResponse": {
                    "type": "object",
                    "required": ["images", "maxImages"],
                    "properties": {
                        "images": { "type": "array", "items": ref_to("GalleryImageWithPlace") },
                        "maxImages": { "type": "integer", "format": "int64", "minimum": 0 }
                    }
                },
                "ForbiddenReason": {
                    "type": "string",
                    "enum": ["maxLimitReached"]
                },
                "ForbiddenError": {
                    "type": "object",
                    "required": ["reason", "message"],
                    "properties": {
                        "reason": ref_to("ForbiddenReason"),
                        "message": { "type": "string" }
                    }
                },
                "ResponseError": {
                    "type": "object",
                    "required": ["message"],
                    "properties": {
                        "message": { "type": "string" }
                    }
                }
            }
        },
        "tags": [
            { "name": "images", "description": "Images management endpoints." }
        ]
    })
}
