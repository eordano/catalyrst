//! Conversation extensions share the same fresh Foundation membership proof as sending.
use super::*;
use std::collections::{HashMap, HashSet};
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Heart,
    Celebrate,
    Pin,
    Unpin,
    Report,
    Dismiss,
    Remove,
}
pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    if !db.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('messages') WHERE name='channel')",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        db.execute_batch(
            "ALTER TABLE messages ADD COLUMN channel TEXT NOT NULL DEFAULT 'general';",
        )?;
    }
    db.execute_batch("CREATE TABLE IF NOT EXISTS channel_messages(seq INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT NOT NULL UNIQUE,community TEXT NOT NULL,wallet TEXT NOT NULL,text TEXT NOT NULL,scene TEXT,created INTEGER NOT NULL,channel TEXT NOT NULL); CREATE INDEX IF NOT EXISTS channel_history ON channel_messages(community,channel,seq); CREATE TABLE IF NOT EXISTS channels(community TEXT NOT NULL, name TEXT NOT NULL, private INTEGER NOT NULL, PRIMARY KEY(community,name)); CREATE TABLE IF NOT EXISTS replies(id TEXT PRIMARY KEY, parent TEXT NOT NULL, wallet TEXT NOT NULL, text TEXT NOT NULL, created INTEGER NOT NULL);
    CREATE INDEX IF NOT EXISTS reply_parent ON replies(parent,created);
    CREATE TABLE IF NOT EXISTS reactions(message TEXT NOT NULL, wallet TEXT NOT NULL, emoji TEXT NOT NULL, PRIMARY KEY(message,wallet,emoji));
    CREATE TABLE IF NOT EXISTS reports(message TEXT NOT NULL,wallet TEXT NOT NULL,created INTEGER NOT NULL,PRIMARY KEY(message,wallet)); CREATE TABLE IF NOT EXISTS pins(message TEXT PRIMARY KEY); CREATE TABLE IF NOT EXISTS channel_config(community TEXT NOT NULL,channel TEXT NOT NULL,config TEXT NOT NULL,PRIMARY KEY(community,channel));")
}
fn check(db: &Connection, community: &str, message: &str) -> ApiResult<()> {
    if !db.query_row("SELECT EXISTS(SELECT 1 FROM (SELECT community,id FROM messages UNION ALL SELECT community,id FROM channel_messages) WHERE community=? AND id=?)", params![community,message], |r| r.get::<_, bool>(0))? {
        return Err(bad("Message does not belong to this community"));
    }
    Ok(())
}
pub fn act(
    s: &Store,
    community: &str,
    message: &str,
    wallet: &str,
    action: &Action,
    role: &str,
) -> ApiResult<()> {
    let mut db = s.db.lock().unwrap();
    let tx = db.transaction()?;
    check(&tx, community, message)?;
    match action {
        Action::Report => {
            tx.execute(
                "INSERT OR IGNORE INTO reports(message,wallet,created) VALUES(?,?,?)",
                params![message, wallet, now()],
            )?;
        }
        Action::Dismiss | Action::Remove => {
            if !matches!(role, "owner" | "moderator") {
                return Err(denied("Only moderators can review reports"));
            }
            if matches!(action, Action::Remove) {
                for table in ["messages", "channel_messages"] {
                    tx.execute(&format!("UPDATE {table} SET text='[Message removed]',scene=NULL WHERE id=? AND community=?"),params![message,community])?;
                }
                tx.execute("DELETE FROM replies WHERE parent=?", [message])?;
                tx.execute("DELETE FROM pins WHERE message=?", [message])?;
            }
            tx.execute("DELETE FROM reports WHERE message=?", [message])?;
        }
        Action::Pin | Action::Unpin => {
            if !matches!(role, "owner" | "moderator") {
                return Err(denied("Only moderators can pin messages"));
            }
            if matches!(action, Action::Pin) {
                tx.execute("INSERT OR IGNORE INTO pins(message) VALUES (?)", [message])?;
            } else {
                tx.execute("DELETE FROM pins WHERE message=?", [message])?;
            }
        }
        _ => {
            let emoji = if matches!(action, Action::Heart) {
                "heart"
            } else {
                "celebrate"
            };
            let removed = tx.execute(
                "DELETE FROM reactions WHERE message=? AND wallet=? AND emoji=?",
                params![message, wallet, emoji],
            )?;
            if removed == 0 {
                tx.execute(
                    "INSERT INTO reactions(message,wallet,emoji) VALUES (?,?,?)",
                    params![message, wallet, emoji],
                )?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}
pub fn reply(
    s: &Store,
    community: &str,
    parent: &str,
    id: &str,
    wallet: &str,
    text: &str,
) -> ApiResult<()> {
    let db = s.db.lock().unwrap();
    let inserted = db.execute(
        "INSERT INTO replies(id,parent,wallet,text,created) SELECT ?,?,?,?,? WHERE EXISTS(SELECT 1 FROM (SELECT community,id FROM messages UNION ALL SELECT community,id FROM channel_messages) WHERE community=? AND id=?)",
        params![id, parent, wallet, text, now(), community, parent],
    )?;
    if inserted == 0 {
        return Err(bad("Message does not belong to this community"));
    }
    Ok(())
}
pub fn enrich(db: &Connection, messages: &mut [Value]) -> ApiResult<()> {
    if messages.is_empty() {
        return Ok(());
    }
    let ids = json!(messages
        .iter()
        .map(|m| m["id"].as_str().unwrap_or(""))
        .collect::<Vec<_>>())
    .to_string();
    let mut replies: HashMap<String, (Vec<Value>, i64)> = HashMap::new();
    let mut stmt = db.prepare("SELECT parent,seq,id,wallet,text,created,total FROM (SELECT parent,rowid AS seq,id,wallet,text,created,ROW_NUMBER() OVER (PARTITION BY parent ORDER BY rowid DESC) AS rn,COUNT(*) OVER (PARTITION BY parent) AS total FROM replies WHERE parent IN (SELECT value FROM json_each(?))) WHERE rn<=100 ORDER BY parent,seq")?;
    for row in stmt.query_map([&ids], |r| {
        Ok((
            r.get::<_, String>(0)?,
            json!({"seq":r.get::<_,i64>(1)?,"id":r.get::<_,String>(2)?,"wallet":r.get::<_,String>(3)?,"text":r.get::<_,String>(4)?,"createdAt":r.get::<_,i64>(5)?}),
            r.get::<_, i64>(6)?,
        ))
    })? {
        let (parent, reply, total) = row?;
        replies
            .entry(parent)
            .or_insert_with(|| (Vec::new(), total))
            .0
            .push(reply);
    }
    let mut reactions: HashMap<String, Vec<Value>> = HashMap::new();
    let mut stmt = db.prepare("SELECT message,emoji,wallet FROM reactions WHERE message IN (SELECT value FROM json_each(?)) ORDER BY message,emoji,wallet")?;
    for row in stmt.query_map([&ids], |r| {
        Ok((
            r.get::<_, String>(0)?,
            json!({"emoji":r.get::<_,String>(1)?,"wallet":r.get::<_,String>(2)?}),
        ))
    })? {
        let (message, reaction) = row?;
        reactions.entry(message).or_default().push(reaction);
    }
    let mut stmt =
        db.prepare("SELECT message FROM pins WHERE message IN (SELECT value FROM json_each(?))")?;
    let pinned: HashSet<String> = stmt
        .query_map([&ids], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for message in messages {
        let id = message["id"].as_str().unwrap_or("").to_owned();
        let (replies, total) = replies.remove(&id).unwrap_or_default();
        message["replies"] = json!(replies);
        message["hasMoreReplies"] = json!(total > 100);
        message["replyCount"] = json!(total);
        message["reactions"] = json!(reactions.remove(&id).unwrap_or_default());
        message["pinned"] = json!(pinned.contains(&id));
    }
    Ok(())
}

pub fn valid_channel(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}
/// The channel's effective configuration, when the operation targets one.
pub fn authorize(
    s: &Store,
    operation: &Operation,
    role: &str,
    wallet: &str,
) -> ApiResult<Option<ChannelConfig>> {
    let moderator = matches!(role, "owner" | "moderator");
    let db = s.db.lock().unwrap();
    let (channel, config, private) = match operation {
        Operation::CommunityReports { .. }
        | Operation::ConfigureChannel { .. }
        | Operation::CreateChannel { .. } => {
            return if moderator {
                Ok(None)
            } else {
                Err(denied("Only moderators can create channels"))
            }
        }
        Operation::OpenCommunity {
            community_id,
            channel,
        }
        | Operation::SendMessage {
            community_id,
            channel,
            ..
        } => {
            let (config, private) = channel_state(&db, community_id, channel)?;
            (channel.clone(), config, private)
        }
        Operation::Reply {
            community_id,
            message_id,
            ..
        }
        | Operation::MessageAction {
            community_id,
            message_id,
            ..
        } => {
            let row: Option<(String, Option<String>, Option<bool>)> = db.query_row("SELECT m.channel,cc.config,ch.private FROM (SELECT channel FROM (SELECT community,id,channel FROM messages UNION ALL SELECT community,id,channel FROM channel_messages) WHERE community=?1 AND id=?2) m LEFT JOIN channel_config cc ON cc.community=?1 AND cc.channel=m.channel LEFT JOIN channels ch ON ch.community=?1 AND ch.name=m.channel", params![community_id,message_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
            let (channel, raw, private) =
                row.ok_or_else(|| bad("Message does not belong to this community"))?;
            (channel, parse_config(raw, private)?, private)
        }
        _ => return Ok(None),
    };
    if config.read_only && !moderator && matches!(operation, Operation::SendMessage { .. }) {
        return Err(denied("Only moderators can post in this channel"));
    }
    if channel == "general" {
        return Ok(Some(config));
    }
    match private {
        Some(false) => Ok(Some(config)),
        Some(true)
            if moderator
                || config
                    .members
                    .iter()
                    .any(|m| m.eq_ignore_ascii_case(wallet)) =>
        {
            Ok(Some(config))
        }
        _ => Err(denied("This channel is restricted or no longer available")),
    }
}
pub fn create_channel(s: &Store, community: &str, name: &str, private: bool) -> ApiResult<()> {
    let db = s.db.lock().unwrap();
    if db.query_row(
        "SELECT COUNT(*) FROM channels WHERE community=?",
        [community],
        |r| r.get::<_, i64>(0),
    )? >= 50
    {
        return Err(bad("This community has reached 50 channels"));
    }
    let inserted = db.execute(
        "INSERT OR IGNORE INTO channels(community,name,private) VALUES (?,?,?)",
        params![community, name, private],
    )?;
    if inserted == 0 {
        return Err(bad("A channel with that name already exists"));
    }
    Ok(())
}
pub fn channels(s: &Store, community: &str, role: &str, wallet: &str) -> ApiResult<Vec<Value>> {
    let db = s.db.lock().unwrap();
    let mut stmt = db.prepare("SELECT ch.name,ch.private,cc.config FROM channels ch LEFT JOIN channel_config cc ON cc.community=ch.community AND cc.channel=ch.name WHERE ch.community=? ORDER BY ch.name")?;
    let rows = stmt
        .query_map([community], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, bool>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut visible = Vec::new();
    for (name, private, raw) in rows {
        let config = parse_config(raw, Some(private))?;
        if !private
            || matches!(role, "owner" | "moderator")
            || config
                .members
                .iter()
                .any(|m| m.eq_ignore_ascii_case(wallet))
        {
            visible.push(json!({"name":name,"private":private,"readOnly":config.read_only}));
        }
    }
    Ok(visible)
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ChannelConfig {
    pub private: bool,
    pub members: Vec<String>,
    pub read_only: bool,
    pub slow_mode: u64,
}
impl ChannelConfig {
    pub fn validate(&self) -> ApiResult<()> {
        if self.members.len() > 100
            || self.slow_mode > 3600
            || self.members.iter().any(|w| {
                w.len() != 42
                    || !w.starts_with("0x")
                    || !w[2..].bytes().all(|c| c.is_ascii_hexdigit())
            })
        {
            return Err(bad(
                "Use valid wallet addresses and a slow mode of up to one hour",
            ));
        }
        Ok(())
    }
}
fn parse_config(raw: Option<String>, private: Option<bool>) -> ApiResult<ChannelConfig> {
    match raw {
        Some(raw) => serde_json::from_str(&raw).map_err(|_| bad("Invalid channel configuration")),
        None => Ok(ChannelConfig {
            private: private.unwrap_or(false),
            ..Default::default()
        }),
    }
}
/// Configuration plus the `channels` row's privacy (`None` when no such row exists).
fn channel_state(
    db: &Connection,
    community: &str,
    channel: &str,
) -> ApiResult<(ChannelConfig, Option<bool>)> {
    let (raw, private): (Option<String>, Option<bool>) = db.query_row(
        "SELECT cc.config,ch.private FROM (SELECT ?1 AS community,?2 AS channel) k LEFT JOIN channel_config cc ON cc.community=k.community AND cc.channel=k.channel LEFT JOIN channels ch ON ch.community=k.community AND ch.name=k.channel",
        params![community, channel],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok((parse_config(raw, private)?, private))
}
fn configuration_db(db: &Connection, community: &str, channel: &str) -> ApiResult<ChannelConfig> {
    Ok(channel_state(db, community, channel)?.0)
}
pub fn configuration(s: &Store, community: &str, channel: &str) -> ApiResult<ChannelConfig> {
    configuration_db(&s.db.lock().unwrap(), community, channel)
}
pub fn configure(
    s: &Store,
    community: &str,
    channel: &str,
    config: &ChannelConfig,
) -> ApiResult<()> {
    config.validate()?;
    if channel == "general" && config.private {
        return Err(bad("General stays open to all community members"));
    }
    let mut db = s.db.lock().unwrap();
    let tx = db.transaction()?;
    if channel != "general"
        && tx.execute(
            "UPDATE channels SET private=? WHERE community=? AND name=?",
            params![config.private, community, channel],
        )? == 0
    {
        return Err(bad("Channel no longer exists"));
    }
    tx.execute("INSERT INTO channel_config(community,channel,config) VALUES(?,?,?) ON CONFLICT(community,channel) DO UPDATE SET config=excluded.config",params![community,channel,serde_json::to_string(config).map_err(|_|bad("Invalid channel configuration"))?])?;
    tx.commit()?;
    Ok(())
}
pub fn check_slow_mode(
    db: &Connection,
    config: Option<ChannelConfig>,
    community: &str,
    channel: &str,
    wallet: &str,
) -> ApiResult<()> {
    let config = match config {
        Some(config) => config,
        None => configuration_db(db, community, channel)?,
    };
    if config.slow_mode == 0 {
        return Ok(());
    }
    let last:Option<i64>=db.query_row("SELECT MAX(created) FROM (SELECT community,channel,wallet,created FROM messages UNION ALL SELECT community,channel,wallet,created FROM channel_messages) WHERE community=? AND channel=? AND wallet=?",params![community,channel,wallet],|r|r.get(0))?;
    if last.is_some_and(|last| now() - last < (config.slow_mode * 1000) as i64) {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            format!(
                "Slow mode: wait {} seconds between messages",
                config.slow_mode
            ),
        ));
    }
    Ok(())
}

pub fn reply_page(
    db: &Connection,
    parent: &str,
    before: Option<i64>,
) -> ApiResult<(Vec<Value>, bool)> {
    let mut statement=db.prepare("SELECT rowid,id,wallet,text,created FROM replies WHERE parent=? AND (? IS NULL OR rowid<?) ORDER BY rowid DESC LIMIT 101")?;
    let mut rows=statement.query_map(params![parent,before,before],|r|Ok(json!({"seq":r.get::<_,i64>(0)?,"id":r.get::<_,String>(1)?,"wallet":r.get::<_,String>(2)?,"text":r.get::<_,String>(3)?,"createdAt":r.get::<_,i64>(4)?})))?.collect::<Result<Vec<_>,_>>()?;
    let more = rows.len() > 100;
    rows.truncate(100);
    rows.reverse();
    Ok((rows, more))
}

pub fn reports(s: &Store, community: &str) -> ApiResult<Vec<Value>> {
    let db = s.db.lock().unwrap();
    let mut stmt=db.prepare("SELECT m.id,m.wallet,m.text,m.channel,COUNT(r.wallet),MIN(r.created) FROM (SELECT id,community,channel,wallet,text FROM messages UNION ALL SELECT id,community,channel,wallet,text FROM channel_messages) m JOIN reports r ON r.message=m.id WHERE m.community=? GROUP BY m.id ORDER BY MIN(r.created) LIMIT 100")?;
    let rows=stmt.query_map([community],|r|Ok(json!({"id":r.get::<_,String>(0)?,"wallet":r.get::<_,String>(1)?,"text":r.get::<_,String>(2)?,"channel":r.get::<_,String>(3)?,"reports":r.get::<_,i64>(4)?})))?.collect::<Result<Vec<_>,_>>()?;
    Ok(rows)
}
