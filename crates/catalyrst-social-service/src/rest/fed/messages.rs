use catalyrst_fed::TypedMessage;
use serde::{Deserialize, Serialize};

fn ec(buf: &mut Vec<u8>, s: &str) {
    buf.extend_from_slice(s.as_bytes());
    buf.push(0);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityCreate {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub unlisted: bool,
    #[serde(default)]
    pub flags: Vec<String>,
}
impl TypedMessage for CommunityCreate {
    const PRIMARY_TYPE: &'static str = "CommunityCreate";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.name);
        ec(&mut b, &self.description);
        b.push(self.private as u8);
        b.push(self.unlisted as u8);
        for f in &self.flags {
            ec(&mut b, f);
        }
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityUpdate {
    pub community_id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub private: Option<bool>,
    pub unlisted: Option<bool>,
}
impl TypedMessage for CommunityUpdate {
    const PRIMARY_TYPE: &'static str = "CommunityUpdate";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.community_id);
        ec(&mut b, self.name.as_deref().unwrap_or(""));
        ec(&mut b, self.description.as_deref().unwrap_or(""));
        b.push(self.private.unwrap_or(false) as u8);
        b.push(self.unlisted.unwrap_or(false) as u8);
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityDelete {
    pub community_id: String,
}
impl TypedMessage for CommunityDelete {
    const PRIMARY_TYPE: &'static str = "CommunityDelete";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.community_id);
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityJoin {
    pub community_id: String,
}
impl TypedMessage for CommunityJoin {
    const PRIMARY_TYPE: &'static str = "CommunityJoin";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.community_id);
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityLeave {
    pub community_id: String,
    pub member: String,
}
impl TypedMessage for CommunityLeave {
    const PRIMARY_TYPE: &'static str = "CommunityLeave";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.community_id);
        ec(&mut b, &self.member);
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityRole {
    pub community_id: String,
    pub target: String,
    pub role: String,
}
impl TypedMessage for CommunityRole {
    const PRIMARY_TYPE: &'static str = "CommunityRole";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.community_id);
        ec(&mut b, &self.target);
        ec(&mut b, &self.role);
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityBan {
    pub community_id: String,
    pub target: String,
    pub reason: Option<String>,
}
impl TypedMessage for CommunityBan {
    const PRIMARY_TYPE: &'static str = "CommunityBan";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.community_id);
        ec(&mut b, &self.target);
        ec(&mut b, self.reason.as_deref().unwrap_or(""));
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityUnban {
    pub community_id: String,
    pub target: String,
}
impl TypedMessage for CommunityUnban {
    const PRIMARY_TYPE: &'static str = "CommunityUnban";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.community_id);
        ec(&mut b, &self.target);
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityPlacesAdd {
    pub community_id: String,
    pub place_ids: Vec<String>,
}
impl TypedMessage for CommunityPlacesAdd {
    const PRIMARY_TYPE: &'static str = "CommunityPlacesAdd";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.community_id);
        for p in &self.place_ids {
            ec(&mut b, p);
        }
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityPlaceRemove {
    pub community_id: String,
    pub place_id: String,
}
impl TypedMessage for CommunityPlaceRemove {
    const PRIMARY_TYPE: &'static str = "CommunityPlaceRemove";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.community_id);
        ec(&mut b, &self.place_id);
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityPost {
    pub community_id: String,
    pub content_hash: String,
}
impl TypedMessage for CommunityPost {
    const PRIMARY_TYPE: &'static str = "CommunityPost";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.community_id);
        ec(&mut b, &self.content_hash);
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityPostDelete {
    pub community_id: String,
    pub post_id: String,
}
impl TypedMessage for CommunityPostDelete {
    const PRIMARY_TYPE: &'static str = "CommunityPostDelete";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.community_id);
        ec(&mut b, &self.post_id);
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityPostLike {
    pub community_id: String,
    pub post_id: String,
}
impl TypedMessage for CommunityPostLike {
    const PRIMARY_TYPE: &'static str = "CommunityPostLike";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.community_id);
        ec(&mut b, &self.post_id);
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityPostUnlike {
    pub community_id: String,
    pub post_id: String,
}
impl TypedMessage for CommunityPostUnlike {
    const PRIMARY_TYPE: &'static str = "CommunityPostUnlike";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.community_id);
        ec(&mut b, &self.post_id);
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityRequest {
    pub community_id: String,
    pub kind: String,
}
impl TypedMessage for CommunityRequest {
    const PRIMARY_TYPE: &'static str = "CommunityRequest";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.community_id);
        ec(&mut b, &self.kind);
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityRequestStatusUpdate {
    pub community_id: String,
    pub request_id: String,
    pub status: String,
}
impl TypedMessage for CommunityRequestStatusUpdate {
    const PRIMARY_TYPE: &'static str = "CommunityRequestStatusUpdate";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.community_id);
        ec(&mut b, &self.request_id);
        ec(&mut b, &self.status);
        b
    }
}
