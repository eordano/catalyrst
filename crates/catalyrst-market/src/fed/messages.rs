use catalyrst_fed::TypedMessage;
use serde::{Deserialize, Serialize};

fn ec(buf: &mut Vec<u8>, s: &str) {
    buf.extend_from_slice(s.as_bytes());
    buf.push(0);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BidPlace {
    pub item_id: String,
    pub price: String,
    pub expires_at: i64,
    #[serde(default)]
    pub fingerprint: String,
    pub signed_at: i64,
}
impl TypedMessage for BidPlace {
    const PRIMARY_TYPE: &'static str = "BidPlace";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.item_id);
        ec(&mut b, &self.price);
        b.extend_from_slice(&self.expires_at.to_be_bytes());
        ec(&mut b, &self.fingerprint);
        b.extend_from_slice(&self.signed_at.to_be_bytes());
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BidCancel {
    pub bid_signature_hash: String,
    pub signed_at: i64,
}
impl TypedMessage for BidCancel {
    const PRIMARY_TYPE: &'static str = "BidCancel";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.bid_signature_hash);
        b.extend_from_slice(&self.signed_at.to_be_bytes());
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BidAccept {
    pub bid_signature_hash: String,
    pub signed_at: i64,
}
impl TypedMessage for BidAccept {
    const PRIMARY_TYPE: &'static str = "BidAccept";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.bid_signature_hash);
        b.extend_from_slice(&self.signed_at.to_be_bytes());
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderCreate {
    pub item_id: String,
    pub price: String,
    pub expires_at: i64,
    pub signed_at: i64,
}
impl TypedMessage for OrderCreate {
    const PRIMARY_TYPE: &'static str = "OrderCreate";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.item_id);
        ec(&mut b, &self.price);
        b.extend_from_slice(&self.expires_at.to_be_bytes());
        b.extend_from_slice(&self.signed_at.to_be_bytes());
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderCancel {
    pub order_signature_hash: String,
    pub signed_at: i64,
}
impl TypedMessage for OrderCancel {
    const PRIMARY_TYPE: &'static str = "OrderCancel";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.order_signature_hash);
        b.extend_from_slice(&self.signed_at.to_be_bytes());
        b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub order_signature_hash: String,
    pub buyer: String,
    pub taken_at: i64,
    pub tx_hash: String,
    pub signed_at: i64,
}
impl TypedMessage for TradeRecord {
    const PRIMARY_TYPE: &'static str = "TradeRecord";
    fn encode_struct(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ec(&mut b, Self::PRIMARY_TYPE);
        ec(&mut b, &self.order_signature_hash);
        ec(&mut b, &self.buyer);
        b.extend_from_slice(&self.taken_at.to_be_bytes());
        ec(&mut b, &self.tx_hash);
        b.extend_from_slice(&self.signed_at.to_be_bytes());
        b
    }
}
