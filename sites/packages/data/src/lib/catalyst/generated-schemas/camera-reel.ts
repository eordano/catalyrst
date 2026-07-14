// GENERATED from catalyrst/ui3/src/generated/catalyst/camera-reel by catalyrst/sites/scripts/gen-zod-schemas.mts. Do not edit.
import { z } from "zod";

import type { GalleryImage } from "@ui/generated/catalyst/camera-reel/GalleryImage";
import type { GalleryImageWithPlace } from "@ui/generated/catalyst/camera-reel/GalleryImageWithPlace";
import type { GetGalleryImagesResponse } from "@ui/generated/catalyst/camera-reel/GetGalleryImagesResponse";
import type { GetImagesResponse } from "@ui/generated/catalyst/camera-reel/GetImagesResponse";
import type { GetMultiplePlacesImagesResponse } from "@ui/generated/catalyst/camera-reel/GetMultiplePlacesImagesResponse";
import type { GetPlaceImagesResponse } from "@ui/generated/catalyst/camera-reel/GetPlaceImagesResponse";
import type { Image } from "@ui/generated/catalyst/camera-reel/Image";
import type { Location } from "@ui/generated/catalyst/camera-reel/Location";
import type { Metadata } from "@ui/generated/catalyst/camera-reel/Metadata";
import type { PlaceDataResponse } from "@ui/generated/catalyst/camera-reel/PlaceDataResponse";
import type { Scene } from "@ui/generated/catalyst/camera-reel/Scene";
import type { UploadResponse } from "@ui/generated/catalyst/camera-reel/UploadResponse";
import type { User } from "@ui/generated/catalyst/camera-reel/User";
import type { UserDataResponse } from "@ui/generated/catalyst/camera-reel/UserDataResponse";

export const GalleryImageSchema = z.object({
  id: z.string(),
  url: z.string(),
  thumbnailUrl: z.string(),
  isPublic: z.boolean(),
  dateTime: z.string(),
});

export const GalleryImageWithPlaceSchema = z.object({
  id: z.string(),
  url: z.string(),
  thumbnailUrl: z.string(),
  isPublic: z.boolean(),
  dateTime: z.string(),
  placeId: z.string(),
});

export const GetGalleryImagesResponseSchema = z.object({
  images: z.array(GalleryImageSchema),
  currentImages: z.number(),
  maxImages: z.number(),
});

export const LocationSchema = z.object({
  x: z.string(),
  y: z.string(),
});

export const SceneSchema = z.object({
  name: z.string(),
  location: LocationSchema,
});

export const UserSchema = z.object({
  userName: z.string(),
  userAddress: z.string(),
  wearables: z.array(z.string()),
  isGuest: z.boolean(),
  isEmoting: z.boolean().nullable(),
});

export const MetadataSchema = z.object({
  userName: z.string(),
  userAddress: z.string(),
  dateTime: z.string(),
  realm: z.string(),
  scene: SceneSchema,
  visiblePeople: z.array(UserSchema),
  placeId: z.string(),
});

export const ImageSchema = z.object({
  id: z.string(),
  url: z.string(),
  thumbnailUrl: z.string(),
  isPublic: z.boolean(),
  metadata: MetadataSchema,
});

export const GetImagesResponseSchema = z.object({
  images: z.array(ImageSchema),
  currentImages: z.number(),
  maxImages: z.number(),
});

export const GetMultiplePlacesImagesResponseSchema = z.object({
  images: z.array(GalleryImageWithPlaceSchema),
  maxImages: z.number(),
});

export const GetPlaceImagesResponseSchema = z.object({
  images: z.array(GalleryImageSchema),
  maxImages: z.number(),
});

export const PlaceDataResponseSchema = z.object({
  maxImages: z.number(),
});

export const UploadResponseSchema = z.object({
  image: ImageSchema,
  currentImages: z.number(),
  maxImages: z.number(),
});

export const UserDataResponseSchema = z.object({
  currentImages: z.number(),
  maxImages: z.number(),
});

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

export type _AssertGalleryImage = Assert<Mutual<GalleryImage, z.infer<typeof GalleryImageSchema>>>;
export type _AssertGalleryImageWithPlace = Assert<Mutual<GalleryImageWithPlace, z.infer<typeof GalleryImageWithPlaceSchema>>>;
export type _AssertGetGalleryImagesResponse = Assert<Mutual<GetGalleryImagesResponse, z.infer<typeof GetGalleryImagesResponseSchema>>>;
export type _AssertGetImagesResponse = Assert<Mutual<GetImagesResponse, z.infer<typeof GetImagesResponseSchema>>>;
export type _AssertGetMultiplePlacesImagesResponse = Assert<Mutual<GetMultiplePlacesImagesResponse, z.infer<typeof GetMultiplePlacesImagesResponseSchema>>>;
export type _AssertGetPlaceImagesResponse = Assert<Mutual<GetPlaceImagesResponse, z.infer<typeof GetPlaceImagesResponseSchema>>>;
export type _AssertImage = Assert<Mutual<Image, z.infer<typeof ImageSchema>>>;
export type _AssertLocation = Assert<Mutual<Location, z.infer<typeof LocationSchema>>>;
export type _AssertMetadata = Assert<Mutual<Metadata, z.infer<typeof MetadataSchema>>>;
export type _AssertPlaceDataResponse = Assert<Mutual<PlaceDataResponse, z.infer<typeof PlaceDataResponseSchema>>>;
export type _AssertScene = Assert<Mutual<Scene, z.infer<typeof SceneSchema>>>;
export type _AssertUploadResponse = Assert<Mutual<UploadResponse, z.infer<typeof UploadResponseSchema>>>;
export type _AssertUser = Assert<Mutual<User, z.infer<typeof UserSchema>>>;
export type _AssertUserDataResponse = Assert<Mutual<UserDataResponse, z.infer<typeof UserDataResponseSchema>>>;
